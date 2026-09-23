use super::*;
use ersatzrs_core::repositories::AddonMediaListRevisionArtifacts;

async fn fixture() -> (
    tempfile::TempDir,
    DbPool,
    EntityLocker,
    Channel,
    DateTime<Utc>,
) {
    let (pool, locker, mut channel, start) = super::tests::fixture().await;
    channel.subtitle_mode = ersatzrs_domain::ChannelSubtitleMode::Default;
    channel.preferred_subtitle_language_code = Some("nld".into());
    make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
          VALUES ('org.example.fixture','1.0.0',0,'fixture','fixture','{}','addons/fixture','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
        UPDATE "RemoteStream" SET "AddonId"='org.example.fixture',"ProviderId"='item' WHERE "Id"=10;
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","PublishedRevision")
          VALUES (1,'org.example.fixture','list','https://example.test/list','Fixture',1);
        INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath") VALUES (1,1,'fixture','streams/list.yml');
        INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness","EffectiveLiveness","Published","MediaItemId")
          VALUES (11,1,'item','Fixture',4,0,'Fixture','https://example.test/finite',1,1,1,10);
        INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","DurationMilliseconds","CreatedAt","UpdatedAt")
          VALUES (1,1,11,'fixture',1,2,'objects/prepared.media',5,60000,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
    "#).execute(&pool).await.unwrap();
    SqliteAddonMediaListRepository(pool.clone())
        .retain_published_revision(
            1,
            1,
            0,
            Utc::now(),
            &[],
            &AddonMediaListRevisionArtifacts {
                ids: vec![1],
                prepared_media: [("item".into(), 1)].into_iter().collect(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    tokio::fs::create_dir(root.path().join("objects"))
        .await
        .unwrap();
    tokio::fs::write(root.path().join("objects/prepared.media"), b"media")
        .await
        .unwrap();
    let channel = make_repositories(pool.clone())
        .channel
        .get_with_profile(channel.id)
        .await
        .unwrap();
    (root, pool, locker, channel, start)
}

async fn published(
    pool: &DbPool,
    locker: &EntityLocker,
    channel: &Channel,
    start: DateTime<Utc>,
) -> super::super::dynamic_projection::Snapshot {
    let schedule_read = schedule::read_locked(
        pool,
        locker,
        channel,
        start,
        start + chrono::Duration::seconds(60),
    )
    .await
    .unwrap()
    .unwrap();
    let profile = playback::load_channel_playback_profile(pool, channel.id)
        .await
        .unwrap()
        .unwrap();
    let result = project_publishable(
        pool,
        channel,
        &schedule_read.items[0],
        &profile.resolution,
        selection::AudioDirective::Published(Some(2)),
    )
    .await
    .unwrap();
    drop(schedule_read);
    result
}

#[tokio::test]
async fn prepared_open_preserves_audio_and_resolves_sidecars_against_actual_artifact() {
    let (root, pool, locker, channel, start) = fixture().await;
    let initial = published(&pool, &locker, &channel, start).await;
    let original_key = initial.identity.cache_key.clone();
    let ready = materialize_prepared(&pool, &locker, initial, Some(2), root.path())
        .await
        .unwrap();
    assert_ne!(ready.identity.cache_key, original_key);
    assert!(ready.prepared_remote.is_none());
    let tracks = ready.projection.item.tracks.unwrap();
    let audio = tracks.audio.unwrap();
    assert_eq!(audio.stream_index, Some(2));
    let Source::Local { path, .. } = audio.source.unwrap() else {
        panic!("expected prepared file")
    };
    assert_eq!(
        std::path::Path::new(&path),
        root.path().join("objects/prepared.media")
    );
    let Source::Local { path, .. } = tracks.subtitle.unwrap().source.unwrap() else {
        panic!("expected sidecar")
    };
    assert_eq!(
        std::path::Path::new(&path),
        root.path().join("objects/fixture.nl.srt")
    );
}

#[tokio::test]
async fn prepared_open_refuses_missing_bytes_and_stale_publication() {
    for removed in [true, false] {
        let (root, pool, locker, channel, start) = fixture().await;
        let initial = published(&pool, &locker, &channel, start).await;
        if removed {
            tokio::fs::remove_file(root.path().join("objects/prepared.media"))
                .await
                .unwrap();
        } else {
            sqlx::query(r#"UPDATE "MediaStream" SET "Title"='Changed track' WHERE "Id"=3"#)
                .execute(&pool)
                .await
                .unwrap();
        }
        assert!(
            materialize_prepared(&pool, &locker, initial, Some(2), root.path())
                .await
                .is_err()
        );
    }
}
