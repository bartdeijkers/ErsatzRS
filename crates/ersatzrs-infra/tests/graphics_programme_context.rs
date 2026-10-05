mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{
    ChannelRepository, GraphicsRepository, GraphicsTextPreviewRequest, PlayoutRepository,
};
use ersatzrs_domain::{
    ChannelPlayoutSource, FillerKind, PlayoutItemGraphicsElement, TextGraphicsRenderer,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor as _;

async fn fixture() -> ersatzrs_infra::db::DbPool {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 10).await;
    let mut mirror = common::channel(11, "11", true);
    mirror.playout_source = ChannelPlayoutSource::Mirror;
    mirror.mirror_source_channel_id = Some(10);
    mirror.playout_offset = Some(Duration::hours(1));
    repos.channel.upsert(&mirror).await.unwrap();
    common::seed_media_graph(&pool).await;
    for id in 1..=3 {
        if id != 1 {
            common::seed_media_item(&pool, id, 1).await;
        }
        sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "MovieMetadata" ("MovieId","DateAdded","DateUpdated","MetadataKind","Title") VALUES (?, '2026-04-15', '2026-04-15', 0, ?)"#)
            .bind(id).bind(format!("Film {id}")).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Width","Height","RFrameRate","VideoScanKind") VALUES (?, ?, '2026-04-15', '2026-04-15', '00:20:00', 1280, 720, '25/1', 0)"#)
            .bind(id).bind(id).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash") VALUES (?, '/media/fixture.mp4', ?)"#)
            .bind(id).bind(id.to_string()).execute(&pool).await.unwrap();
    }
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (20, 10, 'Classic', 0)"#).await.unwrap();
    pool.execute(
        r#"INSERT INTO "GraphicsElement" ("Id", "Name", "Kind") VALUES (100, 'Title', 1)"#,
    )
    .await
    .unwrap();
    for (id, media, begin, end, group, filler, source) in [
        (30, 3, 0, 5, 1, FillerKind::PreRoll, 0),
        (31, 1, 5, 25, 1, FillerKind::None, 0),
        (32, 3, 25, 30, 1, FillerKind::MidRoll, 0),
        (33, 1, 30, 50, 1, FillerKind::None, 20),
        (34, 3, 50, 55, 1, FillerKind::PostRoll, 0),
        (35, 3, 55, 60, 2, FillerKind::PreRoll, 0),
        (36, 2, 60, 80, 2, FillerKind::None, 0),
    ] {
        let mut item = common::playout_item(id, 20, media);
        item.start = common::fixed_time() + Duration::seconds(begin);
        item.finish = common::fixed_time() + Duration::seconds(end);
        item.guide_group = group;
        item.filler_kind = filler;
        item.in_point = Duration::seconds(source);
        item.out_point = item.in_point + (item.finish - item.start);
        repos.playout.upsert_item(&item).await.unwrap();
    }
    pool
}

fn request(seconds: i64) -> GraphicsTextPreviewRequest {
    GraphicsTextPreviewRequest {
        channel_id: 10, element_id: None, render_time: common::fixed_time() + Duration::seconds(seconds),
        template: "{{ CurrentProgramme.Title | default(\"\", true) }}|{{ NextProgramme.Title | default(\"\", true) }}|{{ ContinuingProgramme.Title | default(\"\", true) }}".into(),
        renderer: TextGraphicsRenderer::Drawvg, epg_entries: 8, width: 1280, height: 720,
    }
}

#[tokio::test]
async fn graphics_and_xmltv_do_not_truncate_a_long_split_programme_at_query_padding() {
    let pool = fixture().await;
    pool.execute("DELETE FROM \"PlayoutItem\"").await.unwrap();
    let repos = make_repositories(pool.clone());
    let now = common::fixed_time();
    for (id, start, finish, filler, in_point, out_point) in [
        (
            40,
            now - Duration::hours(20),
            now - Duration::hours(15),
            FillerKind::None,
            Duration::zero(),
            Duration::hours(5),
        ),
        (
            41,
            now - Duration::hours(15),
            now - Duration::hours(15) + Duration::minutes(10),
            FillerKind::MidRoll,
            Duration::zero(),
            Duration::minutes(10),
        ),
        (
            42,
            now - Duration::hours(15) + Duration::minutes(10),
            now + Duration::hours(1),
            FillerKind::None,
            Duration::hours(5),
            Duration::hours(21) - Duration::minutes(10),
        ),
    ] {
        let mut item = common::playout_item(id, 20, if filler == FillerKind::None { 1 } else { 3 });
        item.start = start;
        item.finish = finish;
        item.guide_group = 7;
        item.filler_kind = filler;
        item.in_point = in_point;
        item.out_point = out_point;
        repos.playout.upsert_item(&item).await.unwrap();
    }
    let preview = repos.graphics.preview_text(&request(0)).await.unwrap();
    let start = (now - Duration::hours(20)).to_rfc3339();
    assert_eq!(
        preview.programme_context["CurrentProgramme"]["Start"],
        start
    );
    let mut channel = repos.channel.get_with_profile(10).await.unwrap();
    channel.show_in_epg = true;
    repos.channel.upsert(&channel).await.unwrap();
    let guide =
        ersatzrs_infra::xmltv::load_xmltv_programmes(&pool, now, now + Duration::minutes(10))
            .await
            .unwrap();
    assert_eq!(
        guide
            .iter()
            .find(|item| item.channel_number == "10")
            .unwrap()
            .start,
        now - Duration::hours(20)
    );
}

#[tokio::test]
async fn graphics_preview_uses_historical_full_programmes_and_preserves_the_timeline() {
    let pool = fixture().await;
    let repos = make_repositories(pool.clone());
    let before = repos
        .playout
        .items_in_range(
            20,
            common::fixed_time(),
            common::fixed_time() + Duration::hours(2),
        )
        .await
        .unwrap();
    for (instant, text) in [
        (1, "Film 1|Film 2|"),
        (10, "Film 1|Film 2|"),
        (26, "Film 1|Film 2|Film 1"),
        (52, "Film 1|Film 2|"),
        (56, "Film 2||"),
    ] {
        let preview = repos
            .graphics
            .preview_text(&request(instant))
            .await
            .unwrap();
        assert_eq!(preview.text, text);
    }
    let mut mirrored = request(26);
    mirrored.channel_id = 11;
    mirrored.render_time += Duration::hours(1);
    assert_eq!(
        repos.graphics.preview_text(&mirrored).await.unwrap().text,
        "Film 1|Film 2|Film 1"
    );
    let after = repos
        .playout
        .items_in_range(
            20,
            common::fixed_time(),
            common::fixed_time() + Duration::hours(2),
        )
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn graphics_preview_keeps_raw_epg_and_explicit_overrides_and_rejects_ambiguous_rows() {
    let pool = fixture().await;
    let repos = make_repositories(pool.clone());
    let mut item = common::playout_item(32, 20, 3);
    item.start += Duration::seconds(25);
    item.finish = common::fixed_time() + Duration::seconds(30);
    item.filler_kind = FillerKind::MidRoll;
    item.guide_group = 1;
    item.graphics_elements = vec![PlayoutItemGraphicsElement {
        playout_item_id: 32,
        graphics_element_id: Some(100),
        variables: Some(r#"{"headline":"Manual"}"#.into()),
    }];
    repos.playout.upsert_item(&item).await.unwrap();
    let mut preview = request(26);
    preview.element_id = Some(100);
    preview.template = "{{ headline }}|{{ Epg[0].Title }}|{{ ContinuingProgramme.Title }}|{{ MediaItem_SeekSeconds }}".into();
    assert_eq!(
        repos.graphics.preview_text(&preview).await.unwrap().text,
        "Manual|Film 3|Film 1|1.0"
    );
    sqlx::query(r#"UPDATE "MovieMetadata" SET "Title" = NULL WHERE "MovieId"=2"#)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        repos
            .graphics
            .preview_text(&request(26))
            .await
            .unwrap()
            .text,
        "Film 1||Film 1"
    );
    let mut duplicate = common::playout_item(99, 20, 2);
    duplicate.start = common::fixed_time();
    duplicate.finish = common::fixed_time() + Duration::seconds(40);
    repos.playout.upsert_item(&duplicate).await.unwrap();
    assert!(repos
        .graphics
        .preview_text(&request(26))
        .await
        .unwrap_err()
        .to_string()
        .contains("graphics-preview-ambiguous"));
}

#[tokio::test]
async fn graphics_owned_refresh_adopts_database_variables_for_the_same_source_and_mirror_occurrence(
) {
    use ersatzrs_domain::{
        DrawtextGraphicsConfig, GraphicsElement, GraphicsElementConfig, TextGraphicsConfig,
    };
    use ersatzrs_ffmpeg::{FrameRate, FrameSize};
    use ersatzrs_infra::playback::{
        render_playback_graphics_overlays, PlaybackGraphicsRenderContext,
        ResolvedPlaybackGraphicsElement,
    };
    let pool = fixture().await;
    let items = make_repositories(pool.clone())
        .playout
        .items_in_range(
            20,
            common::fixed_time(),
            common::fixed_time() + Duration::minutes(1),
        )
        .await
        .unwrap();
    let item = items.iter().find(|item| item.id == 31).unwrap();
    pool.execute(r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId", "GraphicsElementId", "Variables") VALUES (31, 100, '{"headline":"Initial"}')"#).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut artifacts = Vec::new();
    for (channel, interval, offset) in [
        (10, 0, Duration::zero()),
        (10, 5, Duration::zero()),
        (11, 5, Duration::hours(1)),
    ] {
        let graphic = ResolvedPlaybackGraphicsElement {
            element: GraphicsElement {
                id: 100,
                config: GraphicsElementConfig::Text(TextGraphicsConfig {
                    renderer: TextGraphicsRenderer::Drawtext,
                    text: Some(
                        "{{ headline | default(\"Native\", true) }}|{{ NextProgramme.Title }}"
                            .into(),
                    ),
                    drawtext: DrawtextGraphicsConfig {
                        metadata_refresh_interval_seconds: Some(interval),
                        refresh_interval_milliseconds: 250,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                ..Default::default()
            },
            variables: Some(r#"{"headline":"Initial"}"#.into()),
        };
        let context = PlaybackGraphicsRenderContext {
            channel_id: channel,
            channel_number: channel.to_string(),
            render_time: common::fixed_time() + Duration::seconds(10) + offset,
            playout_item_id: Some(item.id),
            media: ersatzrs_infra::graphics_template::GraphicsMedia::Persisted(1),
            media_path: "/media/fixture.mp4".into(),
            channel_start_time: item.start + offset,
            content_start_time: item.start + offset,
            content_finish_time: Some(item.finish + offset),
            stream_seek_seconds: 5.0,
            duration_seconds: 20.0,
            frame_size: FrameSize::new(1280, 720),
            square_pixel_frame_size: FrameSize::new(1280, 720),
            frame_rate: Some(FrameRate::DEFAULT.into()),
        };
        artifacts.push(
            render_playback_graphics_overlays(
                &pool,
                &[graphic],
                &context,
                &directory.path().join(format!("{channel}-{interval}")),
            )
            .await
            .unwrap(),
        );
    }
    pool.execute(r#"UPDATE "PlayoutItemGraphicsElement" SET "Variables"='{"headline":"Updated"}' WHERE "PlayoutItemId"=31;
        UPDATE "MovieMetadata" SET "Title"='Next updated' WHERE "MovieId"=2;"#).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5500)).await;
    let read = |index: usize| {
        std::fs::read_to_string(artifacts[index].dynamic_text_controllers[0].text_file()).unwrap()
    };
    assert_eq!(read(0), "Initial|Film 2", "zero keeps the startup snapshot");
    assert_eq!(read(1), "Updated|Next updated");
    assert_eq!(
        read(2),
        "Updated|Next updated",
        "mirror attachments retain source occurrence ownership"
    );
    // Reusing this row identity with altered timing must not update a running
    // graph whose occurrence ended at a different instant.
    pool.execute(r#"UPDATE "PlayoutItem" SET "Finish"='2026-04-15 10:00:24' WHERE "Id"=31;
        UPDATE "PlayoutItemGraphicsElement" SET "Variables"='{"headline":"Wrong occurrence"}' WHERE "PlayoutItemId"=31;"#).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5500)).await;
    assert_eq!(read(1), "Updated|Next updated");
    assert_eq!(read(2), "Updated|Next updated");
    for artifact in artifacts {
        artifact.shutdown().await;
    }
}
