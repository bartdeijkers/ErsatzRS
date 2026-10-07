use super::*;
use chrono::TimeDelta;
use ersatzrs_domain::ChannelPlayoutSource;

async fn fixture() -> (NextHost, Uuid, CanvasBinding, tempfile::TempDir) {
    let (pool, locker, mut channel, start) = schedule::tests::fixture().await;
    channel.streaming_engine = StreamingEngine::Next;
    make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    let media = schedule::read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + TimeDelta::minutes(1),
    )
    .await
    .unwrap()
    .unwrap()
    .items[0]
        .clone();
    let binding = CanvasBinding::new(channel.id, &media, Utc::now() + TimeDelta::hours(1));
    let root = tempfile::tempdir().unwrap();
    let host = NextHost::new(pool, locker, root.path().to_owned());
    let instance = Uuid::new_v4();
    let context = super::super::host::DynamicContext {
        started_at: Utc::now() - TimeDelta::seconds(17),
        request: NextStart {
            channel_id: channel.id,
            number: channel.number,
            preview: false,
            executable: "fixture-next".into(),
            output: root.path().join("output"),
            ffmpeg: "fixture-ffmpeg".into(),
            ffprobe: "fixture-ffprobe".into(),
            replacements: Vec::new(),
            idle_timeout: None,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
        },
        troubleshooting: None,
        transfers: Arc::default(),
        fallback: Arc::default(),
        gaps: Arc::default(),
    };
    host.dynamic_contexts.lock().await.insert(instance, context);
    (host, instance, binding, root)
}

#[tokio::test]
async fn canvas_session_requires_current_item_and_exact_active_process() {
    let (host, instance, binding, _root) = fixture().await;
    let first = host
        .canvas_session(instance, &binding, &canvas_timing())
        .await
        .unwrap();
    let second = host
        .canvas_session(instance, &binding, &canvas_timing())
        .await
        .unwrap();
    assert_eq!(first.started_at, second.started_at);
    assert_eq!(first.timeline.now, binding.start + TimeDelta::seconds(44));
    assert_eq!(first.timeline.content_finish, binding.finish);
    assert!(Arc::ptr_eq(&first.transfers, &second.transfers));
    assert!(first.graphics.is_empty());
    assert!(host
        .canvas_session(Uuid::new_v4(), &binding, &canvas_timing())
        .await
        .is_err());
    let mut wrong_channel = binding.clone();
    wrong_channel.channel += 1;
    assert!(host
        .canvas_session(instance, &wrong_channel, &canvas_timing())
        .await
        .is_err());
    sqlx::query(r#"DELETE FROM "MediaFile" WHERE "Id"=1"#)
        .execute(&host.pool)
        .await
        .unwrap();
    assert!(
        host.canvas_session(instance, &binding, &canvas_timing())
            .await
            .is_err(),
        "the replacement file cannot satisfy the published binding"
    );
    host.dynamic_contexts.lock().await.remove(&instance);
    assert!(host
        .canvas_session(instance, &binding, &canvas_timing())
        .await
        .is_err());
}

#[tokio::test]
async fn canvas_session_reads_mirror_timeline_without_accepting_source_times() {
    let (host, instance, binding, _root) = fixture().await;
    let channel = make_repositories(host.pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    let mirror = Channel {
        id: 2,
        number: "2".into(),
        playout_source: ChannelPlayoutSource::Mirror,
        mirror_source_channel_id: Some(1),
        playout_offset: Some(TimeDelta::hours(1)),
        ..channel
    };
    make_repositories(host.pool.clone())
        .channel
        .upsert(&mirror)
        .await
        .unwrap();
    {
        let mut contexts = host.dynamic_contexts.lock().await;
        let context = contexts.get_mut(&instance).unwrap();
        context.request.channel_id = 2;
        context.request.number = "2".into();
    }
    let mut shifted = binding;
    shifted.channel = 2;
    shifted.start += TimeDelta::hours(1);
    shifted.finish += TimeDelta::hours(1);
    let session = host
        .canvas_session(instance, &shifted, &canvas_timing())
        .await
        .unwrap();
    assert_eq!(session.media.item.start, shifted.start);
    assert_eq!(session.media.item.finish, shifted.finish);
    assert_eq!(session.timeline.now, shifted.start + TimeDelta::seconds(44));
    shifted.start -= TimeDelta::hours(1);
    shifted.finish -= TimeDelta::hours(1);
    assert!(host
        .canvas_session(instance, &shifted, &canvas_timing())
        .await
        .is_err());
}

#[tokio::test]
async fn canvas_session_preview_is_transient_and_instance_specific() {
    let (host, instance, _, _root) = fixture().await;
    let channel = make_repositories(host.pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    let profile = playback::load_channel_playback_profile(&host.pool, 1)
        .await
        .unwrap()
        .unwrap();
    let media = schedule::bind_media(
        &host.pool,
        ersatzrs_domain::PlayoutItem {
            media_item_id: 10,
            start: Utc::now(),
            finish: Utc::now() + TimeDelta::minutes(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let context = super::super::troubleshooting::Context::new(
        &super::super::troubleshooting::TroubleshootingPreview {
            media: &media,
            channel: &channel,
            profile: &profile,
            subtitle_id: None,
            selection_time: None,
            watermark_ids: &[],
            graphics_ids: &[],
            replacements: &[],
            ffmpeg: std::path::Path::new("fixture-ffmpeg"),
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
            diagnostics: None,
        },
    );
    let binding =
        CanvasBinding::new(1, &media, Utc::now() + TimeDelta::hours(1)).for_session(instance);
    {
        let mut contexts = host.dynamic_contexts.lock().await;
        let session = contexts.get_mut(&instance).unwrap();
        session.request.preview = true;
        session.troubleshooting = Some(Arc::new(super::super::troubleshooting::Session::single(
            Arc::new(context),
        )));
    }
    let selected = host
        .canvas_session(instance, &binding, &canvas_timing())
        .await
        .unwrap();
    assert_eq!(selected.media.item.id, 0);
    assert!(selected.graphics.is_empty());
    let other = Uuid::new_v4();
    {
        let mut contexts = host.dynamic_contexts.lock().await;
        let original = contexts.get(&instance).unwrap();
        let duplicate = super::super::host::DynamicContext {
            started_at: original.started_at,
            request: original.request.clone(),
            troubleshooting: original.troubleshooting.clone(),
            transfers: Arc::default(),
            fallback: Arc::default(),
            gaps: Arc::default(),
        };
        contexts.insert(other, duplicate);
    }
    assert!(
        host.canvas_session(other, &binding, &canvas_timing())
            .await
            .is_err(),
        "identical preview media does not authorize a different process"
    );
    host.dynamic_contexts
        .lock()
        .await
        .get_mut(&instance)
        .unwrap()
        .troubleshooting = None;
    assert!(
        host.canvas_session(instance, &binding, &canvas_timing())
            .await
            .is_err(),
        "a transient preview must not resolve as an ordinary session item"
    );
    let rows: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem" WHERE "Id"=0"#)
        .fetch_one(&host.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

fn canvas_timing() -> super::super::canvas::CanvasRequest {
    super::super::canvas::CanvasRequest::parse("44000", "44000", "25").unwrap()
}

#[tokio::test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
#[allow(
    clippy::disallowed_methods,
    reason = "manual package gate supplies the verified executable"
)]
async fn canvas_session_renders_selected_script_and_transparent_removed_graphics() {
    use ersatzrs_core::repositories::GraphicsRepository as _;
    use ersatzrs_domain::{
        GraphicsElement, GraphicsElementConfig, GraphicsElementKind, ScriptGraphicsConfig,
    };
    use tokio::io::AsyncReadExt as _;

    let executable = std::env::var("ERSATZRS_FFMPEG").expect("packaged FFmpeg executable");
    let (host, instance, binding, root) = fixture().await;
    host.dynamic_contexts
        .lock()
        .await
        .get_mut(&instance)
        .unwrap()
        .request
        .ffmpeg = (&executable).into();
    sqlx::query(r#"UPDATE "Resolution" SET "Width"=64, "Height"=64 WHERE "Id"=1"#)
        .execute(&host.pool)
        .await
        .unwrap();
    make_repositories(host.pool.clone())
        .graphics
        .upsert_element(&GraphicsElement {
            id: 901,
            name: "Canvas script".into(),
            kind: GraphicsElementKind::Script,
            config: GraphicsElementConfig::Script(ScriptGraphicsConfig {
                command: Some("return 'setcolor red\\nrect 8 8 16 16 fill\\n'".into()),
                ..ScriptGraphicsConfig::default()
            }),
            ..GraphicsElement::default()
        })
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId", "GraphicsElementId") VALUES (?, 901)"#)
        .bind(binding.item).execute(&host.pool).await.unwrap();
    let timing = CanvasRequest::parse("0", "1000", "4.0").unwrap();
    for selected in [true, false] {
        if !selected {
            sqlx::query(
                r#"DELETE FROM "PlayoutItemGraphicsElement" WHERE "GraphicsElementId"=901"#,
            )
            .execute(&host.pool)
            .await
            .unwrap();
        }
        let mut reader = host
            .render_canvas(instance, &binding, &timing)
            .await
            .unwrap();
        let mut bytes = Vec::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            reader.read_to_end(&mut bytes),
        )
        .await
        .unwrap()
        .unwrap();
        let nut = root.path().join("canvas.nut");
        std::fs::write(&nut, bytes).unwrap();
        let decoded = tokio::process::Command::new(&executable)
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(nut)
            .args(["-pix_fmt", "bgra", "-f", "rawvideo", "pipe:1"])
            .kill_on_drop(true)
            .output()
            .await
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        assert_eq!(decoded.stdout.len(), 64 * 64 * 4 * 4);
        for frame in decoded.stdout.chunks_exact(64 * 64 * 4) {
            assert_eq!(frame[3], 0);
            let pixel = &frame[(12 * 64 + 12) * 4..(12 * 64 + 12) * 4 + 4];
            if selected {
                assert_eq!(pixel, &[0, 0, 255, 255]);
            } else {
                assert_eq!(pixel[3], 0);
            }
        }
    }
    host.dynamic_contexts
        .lock()
        .await
        .remove(&instance)
        .unwrap()
        .transfers
        .shutdown()
        .await
        .unwrap();
    assert!(host
        .render_canvas(instance, &binding, &timing)
        .await
        .is_err());
}

async fn seed_timed_decorations(host: &NextHost) {
    use ersatzrs_core::repositories::GraphicsRepository as _;
    use ersatzrs_domain::GraphicsElement;

    make_repositories(host.pool.clone())
        .graphics
        .upsert_element(&GraphicsElement {
            id: 901,
            name: "Chunk decoration".into(),
            ..GraphicsElement::default()
        })
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"
        INSERT INTO "DecoGroup" ("Id", "Name") VALUES (901, 'canvas');
        INSERT INTO "Deco" (
            "Id", "DecoGroupId", "Name", "BreakContentMode", "DeadAirFallbackCollectionType",
            "DeadAirFallbackMode", "DefaultFillerCollectionType", "DefaultFillerMode",
            "DefaultFillerTrimToFit", "GraphicsElementsMode", "UseGraphicsElementsDuringFiller",
            "UseWatermarkDuringFiller", "WatermarkMode"
        ) VALUES (901, 901, 'canvas', 0, 0, 0, 0, 0, 0, 2, 1, 1, 2);
        INSERT INTO "DecoGraphicsElement" ("DecoId", "GraphicsElementId") VALUES (901, 901);
        INSERT INTO "ChannelWatermark" (
            "Id", "Name", "Mode", "ImageSource", "Image", "Location", "Size",
            "WidthPercent", "HorizontalMarginPercent", "VerticalMarginPercent",
            "FrequencyMinutes", "DurationSeconds", "Opacity", "PlaceWithinSourceContent", "ZIndex"
        ) VALUES (901, 'canvas', 1, 1, NULL, 0, 0, 10, 0, 0, 0, 0, 100, 0, 0);
        INSERT INTO "Artwork" ("Id", "ArtworkKind", "ChannelId", "Path", "DateAdded", "DateUpdated")
            VALUES (901, 2, 1, 'https://example.test/canvas.png', '2026-09-22', '2026-09-22');
        INSERT INTO "DecoWatermark" ("DecoId", "WatermarkId") VALUES (901, 901);
        INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (901, 'canvas');
        INSERT INTO "Template" ("Id", "TemplateGroupId", "Name", "DateUpdated")
            VALUES (901, 901, 'canvas', '2026-09-22 00:00:00');
        INSERT INTO "DecoTemplateGroup" ("Id", "Name") VALUES (901, 'canvas');
        INSERT INTO "DecoTemplate" ("Id", "DecoTemplateGroupId", "Name", "DateUpdated")
            VALUES (901, 901, 'canvas', '2026-09-22 00:00:00');
        INSERT INTO "DecoTemplateItem" ("Id", "DecoTemplateId", "DecoId", "StartTime", "EndTime")
            VALUES (901, 901, 901, '12:00:30', '12:01:00');
        INSERT INTO "PlayoutTemplate" (
            "Id", "PlayoutId", "TemplateId", "DateUpdated", "DecoTemplateId", "Index",
            "LimitToDateRange", "StartMonth", "StartDay", "EndMonth", "EndDay"
        ) VALUES (901, 1, 901, '2026-09-22 00:00:00', 901, 0, 0, 1, 1, 12, 31);
        "#,
    )
    .execute(&host.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn canvas_session_selects_decorations_at_requested_chunk_time() {
    let (host, instance, binding, _root) = fixture().await;
    seed_timed_decorations(&host).await;
    let initial = CanvasRequest::parse("0", "44000", "25").unwrap();
    assert!(host
        .canvas_session(instance, &binding, &initial)
        .await
        .unwrap()
        .graphics
        .is_empty());
    let later = host
        .canvas_session(instance, &binding, &canvas_timing())
        .await
        .unwrap();
    assert_eq!(later.graphics.len(), 1);
    assert_eq!(later.graphics[0].element.id, 901);
    assert_eq!(later.timeline.duration, std::time::Duration::from_secs(16));
}

#[tokio::test]
async fn canvas_channel_preview_advances_decorations_on_source_and_mirror_clocks() {
    for mirrored in [false, true] {
        let (host, instance, original, _root) = fixture().await;
        seed_timed_decorations(&host).await;
        let repos = make_repositories(host.pool.clone());
        let mut channel = repos.channel.get_with_profile(1).await.unwrap();
        let mirror_offset = if mirrored {
            TimeDelta::hours(1)
        } else {
            TimeDelta::zero()
        };
        if mirrored {
            channel.id = 2;
            channel.number = "2".into();
            channel.playout_source = ChannelPlayoutSource::Mirror;
            channel.mirror_source_channel_id = Some(1);
            channel.playout_offset = Some(mirror_offset);
            repos.channel.upsert(&channel).await.unwrap();
            sqlx::query(r#"INSERT INTO "Artwork" ("Id", "ArtworkKind", "ChannelId", "Path", "DateAdded", "DateUpdated") VALUES (902, 2, 2, 'https://example.test/mirror.png', '2026-09-22', '2026-09-22')"#)
                .execute(&host.pool).await.unwrap();
        }
        let initial = host
            .canvas_session(
                instance,
                &original,
                &CanvasRequest::parse("0", "1000", "25").unwrap(),
            )
            .await
            .unwrap();
        let mut media = initial.media;
        let selected_at = original.start + mirror_offset + TimeDelta::seconds(10);
        // The preview is played now, while the operator selected a historical occurrence.
        media.item.start = Utc::now();
        media.item.finish = media.item.start + TimeDelta::seconds(30);
        let preview = super::super::troubleshooting::Context::new(
            &super::super::troubleshooting::TroubleshootingPreview {
                media: &media,
                channel: &channel,
                profile: &initial.profile,
                subtitle_id: None,
                selection_time: Some(selected_at),
                watermark_ids: &[],
                graphics_ids: &[],
                replacements: &[],
                ffmpeg: std::path::Path::new("fixture-ffmpeg"),
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
                diagnostics: None,
            },
        );
        let binding = CanvasBinding::new(channel.id, &media, Utc::now() + TimeDelta::hours(1))
            .for_session(instance);
        {
            let mut contexts = host.dynamic_contexts.lock().await;
            let context = contexts.get_mut(&instance).unwrap();
            context.request.channel_id = channel.id;
            context.request.number = channel.number;
            context.request.preview = true;
            context.troubleshooting = Some(Arc::new(
                super::super::troubleshooting::Session::single(Arc::new(preview)),
            ));
        }
        for (offset, count) in [("0", 0), ("19000", 0), ("20000", 1)] {
            let selected = host
                .canvas_session(
                    instance,
                    &binding,
                    &CanvasRequest::parse(offset, "1000", "25").unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                selected.graphics.len(),
                count,
                "graphics mirrored={mirrored} offset={offset}"
            );
            assert_eq!(
                selected.watermarks.len(),
                count,
                "watermarks mirrored={mirrored} offset={offset}"
            );
        }
    }
}

#[tokio::test]
async fn canvas_session_restores_programme_offset_after_dynamic_item_resolution() {
    let (host, instance, mut binding, _root) = fixture().await;
    binding.playout_start = Some(binding.start + TimeDelta::seconds(10));
    binding.video_stream_index = Some(0);
    let request = CanvasRequest::parse("5000", "1000", "25").unwrap();
    let session = host
        .canvas_session(instance, &binding, &request)
        .await
        .unwrap();
    assert_eq!(session.timeline.content_start, binding.start);
    assert_eq!(session.timeline.content_finish, binding.finish);
    assert_eq!(session.timeline.now, binding.start + TimeDelta::seconds(15));
    assert_eq!(session.video_stream_index, Some(0));
    for start in [binding.start - TimeDelta::milliseconds(1), binding.finish] {
        binding.playout_start = Some(start);
        assert!(host
            .canvas_session(instance, &binding, &request)
            .await
            .is_err());
    }
}
