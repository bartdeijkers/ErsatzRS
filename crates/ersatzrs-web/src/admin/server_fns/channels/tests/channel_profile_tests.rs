use super::*;
use ersatzrs_app::commands::{SaveFfmpegProfile, SaveFfmpegProfileForApi};
use ersatzrs_core::repositories::{
    ChannelRepository as _, FfmpegProfileRepository as _, PlayoutRepository as _,
};
use ersatzrs_domain::{Channel, FFmpegProfile, FFmpegProfileVideoFormat};
use std::sync::{atomic::Ordering, Arc};

#[cfg(test)]
async fn seed(state: &crate::host::state::AppState) {
    let repos = &state.repositories;
    for id in [1, 2] {
        repos
            .ffmpeg_profile
            .upsert(&FFmpegProfile {
                id,
                name: format!("Profile {id}"),
                resolution_id: 1,
                ..FFmpegProfile::default()
            })
            .await
            .unwrap();
    }
    repos
        .channel
        .upsert(&Channel {
            id: 1,
            number: "1".into(),
            name: "Synthetic".into(),
            ffmpeg_profile_id: 1,
            ..Channel::default()
        })
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (1,1,0,123)"#,
    )
    .execute(&state.pool)
    .await
    .unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Fixture',1,1);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'synthetic');
        INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (1,1,0);
        INSERT INTO "PlayoutItem" ("Id","PlayoutId","MediaItemId","Start","Finish","InPoint","OutPoint","DisableWatermarks","FillerKind","GuideGroup")
        VALUES (1,1,1,'2026-10-06 12:00:00','2026-10-06 12:05:00','00:00:17','00:05:17',0,0,11);
    "#).execute(&state.pool).await.unwrap();
}

async fn snapshot(state: &crate::host::state::AppState) -> Vec<ersatzrs_domain::PlayoutItem> {
    use chrono::TimeZone as _;
    let start = chrono::Utc.with_ymd_and_hms(2026, 10, 6, 12, 0, 0).unwrap();
    state
        .repositories
        .playout
        .items_in_range(1, start, start + chrono::Duration::minutes(10))
        .await
        .unwrap()
}

#[cfg(test)]
#[tokio::test]
async fn profile_copy_and_resolution_changes_notify_consumers_without_rebuilding() {
    let (state, mut builds) = migrated_channel_state_with_playout_rx().await;
    seed(&state).await;
    let before = snapshot(&state).await;
    let repos = &state.repositories;
    let normal = Arc::new(UpdateObserver::default());
    let preview = Arc::new(UpdateObserver::default());
    state.hls_sessions.insert(normal.clone()).await;
    state.preview_hls_sessions.insert(preview.clone()).await;
    for (id, transcode, resolution, api, engine, expected) in [
        (1, false, 1, false, 1, 1), // editor: enter copy
        (1, false, 1, true, 1, 1),  // API: unchanged projection
        (2, false, 1, false, 1, 1), // another profile
        (1, true, 1, true, 1, 2),   // API: leave copy
        (1, true, 2, false, 1, 3),  // editor: resize canvas
        (1, false, 2, true, 0, 3),  // native engine needs no Next publication
    ] {
        sqlx::query(r#"UPDATE "Channel" SET "StreamingEngine"=? WHERE "Id"=1"#)
            .bind(engine)
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT OR IGNORE INTO "Resolution" ("Id","Name","Width","Height") VALUES (2,'Fixture',640,360)"#)
            .execute(&state.pool).await.unwrap();
        let mut form = state
            .application
            .get_ffmpeg_profile(id)
            .await
            .unwrap()
            .unwrap();
        form.transcode_video = transcode;
        form.video_format = 1;
        form.resolution_id = resolution;
        form.name = format!("Profile {id} renamed");
        if api {
            state
                .application
                .save_ffmpeg_profile_for_api(SaveFfmpegProfileForApi {
                    form,
                    is_update: true,
                    qsv_extra_hardware_frames: None,
                })
                .await
                .unwrap();
        } else {
            state
                .application
                .save_ffmpeg_profile(SaveFfmpegProfile { form })
                .await
                .unwrap();
        }
        assert_eq!(normal.0.load(Ordering::SeqCst), expected);
        assert_eq!(preview.0.load(Ordering::SeqCst), expected);
        assert!(
            builds.try_recv().is_err(),
            "profile saves must not rebuild the schedule"
        );
        let seed: i64 = sqlx::query_scalar(r#"SELECT "Seed" FROM "Playout" WHERE "Id"=1"#)
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(seed, 123);
        assert_eq!(snapshot(&state).await, before);
        assert_eq!(
            repos.ffmpeg_profile.get(id).await.unwrap().video_format,
            if transcode {
                FFmpegProfileVideoFormat::H264
            } else {
                FFmpegProfileVideoFormat::Copy
            }
        );
    }
}
