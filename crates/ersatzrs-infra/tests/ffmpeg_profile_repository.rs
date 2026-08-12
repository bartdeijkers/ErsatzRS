#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, MediaItemRepository, PlayoutRepository,
    ProgramScheduleRepository,
};
use ersatzrs_domain::ffmpeg::{ContentLightLevel, MasteringDisplayMetadata};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor as _;

/// Regression for the 2026-08-12 data loss: editing an `FFmpeg` profile deleted
/// every channel that used it.
///
/// `upsert` used `REPLACE INTO`, which deletes the conflicting row before
/// inserting the replacement. With foreign keys enabled,
/// `FK_Channel_FFmpegProfile_FFmpegProfileId ... ON DELETE CASCADE` then
/// removed every channel on that profile, cascading on to its playouts and
/// playout items, while the profile itself looked untouched.
#[tokio::test]
async fn editing_a_profile_does_not_cascade_delete_its_channels() {
    let repos = make_repositories(common::test_db().await);
    common::seed_profile_and_channel(&repos, 7).await;
    assert_eq!(
        repos.channel.list_enabled().await.unwrap().len(),
        1,
        "precondition: the seeded channel uses profile 1"
    );

    // Exactly what the admin UI does when a profile is edited and saved.
    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "edited"))
        .await
        .unwrap();

    assert_eq!(
        repos.ffmpeg_profile.get(1).await.unwrap().name,
        "edited",
        "the edit must still be persisted"
    );
    assert_eq!(
        repos.channel.list_enabled().await.unwrap().len(),
        1,
        "editing a profile must not delete channels that reference it"
    );
}

/// Companion to the profile regression: `Playout` and `Artwork` cascade off
/// `Channel`, so the same `REPLACE INTO` hazard meant saving a channel
/// destroyed its playout.
#[tokio::test]
async fn saving_a_channel_does_not_cascade_delete_its_playout() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 7).await;
    repos
        .program_schedule
        .upsert(&common::schedule(1, Vec::new()))
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
         VALUES (1, 7, 1, 'Classic', 0)"#,
    )
    .await
    .unwrap();
    assert!(
        repos.playout.get_with_anchor(1).await.is_ok(),
        "precondition: channel 7 has a playout"
    );

    let mut channel = common::channel(7, "7", true);
    channel.name = "renamed".to_owned();
    repos.channel.upsert(&channel).await.unwrap();

    assert!(
        repos.playout.get_with_anchor(1).await.is_ok(),
        "saving a channel must not delete its playout"
    );
}

/// `MediaItem` is the widest cascade parent in the schema — 21 tables cascade
/// off it, including `Episode`, `Movie` and `PlayoutItem` — and it is rewritten
/// for every scanned item, so a `REPLACE` here erased library state on any
/// rescan.
#[tokio::test]
async fn re_upserting_a_media_item_does_not_cascade_delete_its_children() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_media_graph(&pool).await;

    let item = common::media_item(900, 1);
    repos.media_item.upsert(&item).await.unwrap();
    pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (900)"#)
        .await
        .unwrap();

    // Re-scanning the same item runs the same upsert again.
    repos.media_item.upsert(&item).await.unwrap();

    let movies: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Movie" WHERE "Id" = 900"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        movies, 1,
        "re-upserting a media item must not delete its child rows"
    );
}

#[tokio::test]
async fn ffmpeg_profile_repository_round_trips_lists_and_deletes() {
    let repos = make_repositories(common::test_db().await);

    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "default"))
        .await
        .unwrap();

    assert_eq!(repos.ffmpeg_profile.get(1).await.unwrap().name, "default");
    assert_eq!(repos.ffmpeg_profile.list().await.unwrap().len(), 1);

    repos.ffmpeg_profile.delete(1).await.unwrap();
    assert!(repos.ffmpeg_profile.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn ffmpeg_profile_admin_create_copy_and_absence_are_typed() {
    let repos = make_repositories(common::test_db().await);
    let mut profile = common::profile(0, "admin");
    let id = repos.ffmpeg_profile.save_admin(&profile).await.unwrap();
    assert!(id > 0);
    assert!(repos.ffmpeg_profile.name_exists(0, "admin").await.unwrap());
    assert_eq!(
        repos
            .ffmpeg_profile
            .get_optional_admin(id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "admin"
    );

    let copy_id = repos
        .ffmpeg_profile
        .copy_admin(id, "admin copy")
        .await
        .unwrap();
    assert_ne!(copy_id, id);
    assert_eq!(
        repos.ffmpeg_profile.get(copy_id).await.unwrap().name,
        "admin copy"
    );
    assert!(repos
        .ffmpeg_profile
        .get_optional_admin(99_999)
        .await
        .unwrap()
        .is_none());

    profile.id = id;
    profile.name = "updated".to_owned();
    assert_eq!(repos.ffmpeg_profile.save_admin(&profile).await.unwrap(), id);
}

#[tokio::test]
async fn ffmpeg_profile_admin_reads_report_closed_pool_errors() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;
    assert!(repos.ffmpeg_profile.list_resolutions().await.is_err());
    assert!(repos.ffmpeg_profile.name_exists(0, "name").await.is_err());
    assert!(repos.ffmpeg_profile.get_optional_admin(1).await.is_err());
}

#[tokio::test]
async fn hdr10_static_metadata_overrides_survive_a_round_trip_and_a_copy() {
    // ADR 0064: the values ErsatzRS signals must be editable, which means they
    // have to persist. NULL columns keep meaning "use the documented default".
    let repos = make_repositories(common::test_db().await);

    let mut profile = common::profile(1, "default");
    assert_eq!(
        profile.hdr_mastering_display, None,
        "a profile that never set an override must read back as the documented default"
    );

    let operator_grade = MasteringDisplayMetadata {
        green_x: 13_250,
        green_y: 34_500,
        blue_x: 7_500,
        blue_y: 3_000,
        red_x: 34_000,
        red_y: 16_000,
        white_point_x: 15_635,
        white_point_y: 16_450,
        max_luminance: 10_000_000,
        min_luminance: 50,
    };
    profile.hdr_mastering_display = Some(operator_grade);
    profile.hdr_content_light = ContentLightLevel::new(600, 120);
    repos.ffmpeg_profile.upsert(&profile).await.unwrap();

    let stored = repos.ffmpeg_profile.get(1).await.unwrap();
    assert_eq!(stored.hdr_mastering_display, Some(operator_grade));
    assert_eq!(stored.hdr_content_light, ContentLightLevel::new(600, 120));

    // Copying a profile must carry the grade with it, or the copy would
    // silently signal different metadata than the profile it came from.
    let copy_id = repos
        .ffmpeg_profile
        .copy_admin(1, "graded copy")
        .await
        .unwrap();
    let copied = repos.ffmpeg_profile.get(copy_id).await.unwrap();
    assert_eq!(copied.hdr_mastering_display, Some(operator_grade));
    assert_eq!(copied.hdr_content_light, ContentLightLevel::new(600, 120));

    // Clearing the override returns to the documented default rather than
    // leaving the previous value behind.
    profile.hdr_mastering_display = None;
    profile.hdr_content_light = None;
    repos.ffmpeg_profile.upsert(&profile).await.unwrap();
    let cleared = repos.ffmpeg_profile.get(1).await.unwrap();
    assert_eq!(cleared.hdr_mastering_display, None);
    assert_eq!(cleared.hdr_content_light, None);
}
