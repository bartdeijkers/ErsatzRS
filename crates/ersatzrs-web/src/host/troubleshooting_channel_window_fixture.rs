//! Disposable scheduled input, exact pins, fallback, Mirror and operator credits template.
use super::*;
use ersatzrs_core::repositories::GraphicsRepository as _;
use ersatzrs_domain::{
    ChannelMusicVideoCreditsMode, ChannelPlayoutSource, DrawtextGraphicsConfig, GraphicsElement,
    GraphicsElementConfig, ScriptGraphicsConfig, StyleDefinition, TextGraphicsConfig,
    TextGraphicsRenderer, WatermarkLocation,
};

pub(super) async fn configure(
    fixture: &Fixture,
    scenario: Scenario,
    origin: &origin::Origin,
) -> PlaybackPreviewQuery {
    setup(fixture, scenario).await;
    let mut query = Fixture::query();
    query.media_item = None;
    query.seek_seconds = None;
    query.channel = Some(1);
    query.preferred_audio_title = Some("Dutch".into());
    query.subtitle_id = None;
    query.start = Some(fixture.at.to_rfc3339());
    if scenario.timeline() {
        configure_timeline(fixture, scenario, origin, &mut query).await;
    } else {
        configure_credits(fixture, scenario, origin, &mut query).await;
    }
    query
}

async fn setup(fixture: &Fixture, scenario: Scenario) {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    {
        let mut runtime = fixture.state.ffmpeg_runtime.write().await;
        runtime.ffmpeg.path = ffmpeg.to_str().unwrap().into();
        runtime.ffprobe.path = ffprobe.to_str().unwrap().into();
    }
    channel_window_second_movie(fixture).await;
    generate(
        &ffmpeg,
        &fixture.root.path().join("first.mkv"),
        "blue",
        25,
        880,
        true,
    )
    .await;
    generate(
        &ffmpeg,
        &fixture.root.path().join("fixture.mkv"),
        if scenario.timeline() { "red" } else { "blue" },
        30,
        if scenario.timeline() { 1760 } else { 880 },
        scenario.finite_producer(),
    )
    .await;
    generate(
        &ffmpeg,
        &fixture.root.path().join("filler.mkv"),
        "lime",
        25,
        220,
        false,
    )
    .await;
    seed_filler(fixture).await;
    for sql in [
        r#"UPDATE "MediaVersion" SET "Duration"='00:01:30',"RFrameRate"='25' WHERE "Id" IN (100,101)"#,
        r#"UPDATE "MediaVersion" SET "Duration"='00:01:30',"RFrameRate"='30' WHERE "Id"=200"#,
        r#"UPDATE "MediaStream" SET "Channels"=1 WHERE "MediaStreamKind"=2"#,
        r#"DELETE FROM "PlayoutItem""#,
    ] {
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
    }
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
        .bind(fixture.root.path().join("first.mkv").to_str().unwrap())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    seed_profile_and_channel(fixture, scenario).await;
}

async fn configure_timeline(
    fixture: &Fixture,
    scenario: Scenario,
    origin: &origin::Origin,
    query: &mut PlaybackPreviewQuery,
) {
    timeline_items(fixture, scenario.normalize()).await;
    remote(
        fixture,
        11,
        200,
        &format!("{}/source.mkv", origin.base),
        false,
    )
    .await;
    attach_canvas(fixture, 42).await;
    attach_source_clock(fixture, 42).await;
    query.subtitle_id = Some(0);
    let mut mirror = fixture
        .state
        .repositories
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    mirror.id = 2;
    mirror.number = "2".into();
    mirror.playout_source = ChannelPlayoutSource::Mirror;
    mirror.mirror_source_channel_id = Some(1);
    mirror.playout_offset = Some(TimeDelta::hours(1));
    fixture
        .state
        .repositories
        .channel
        .upsert(&mirror)
        .await
        .unwrap();
    query.channel = Some(2);
    query.start =
        Some((fixture.at + TimeDelta::hours(1) + TimeDelta::milliseconds(789)).to_rfc3339());
}

async fn configure_credits(
    fixture: &Fixture,
    scenario: Scenario,
    origin: &origin::Origin,
    query: &mut PlaybackPreviewQuery,
) {
    credit_items(fixture, scenario).await;
    remote(
        fixture,
        10,
        100,
        &format!("{}/source.mkv", origin.base),
        scenario.live(),
    )
    .await;
    music_video(fixture, scenario).await;
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
        .bind(fixture.root.path().join("fixture.mkv").to_str().unwrap())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    if scenario.producer() {
        producer::install(fixture, scenario.live()).await;
    }
    if scenario.custom() {
        let mut channel = fixture
            .state
            .repositories
            .channel
            .get_with_profile(1)
            .await
            .unwrap();
        channel.stream_selector_mode = ersatzrs_domain::ChannelStreamSelectorMode::Custom;
        let selector = "row201-caption.yml";
        tokio::fs::create_dir_all(channel_stream_selectors_folder())
            .await
            .unwrap();
        tokio::fs::write(channel_stream_selectors_folder().join(selector), b"items:\n  - audio_language: [nld]\n    subtitle_language: [nld]\n    subtitle_condition: default\n").await.unwrap();
        channel.stream_selector = Some(selector.into());
        fixture
            .state
            .repositories
            .channel
            .upsert(&channel)
            .await
            .unwrap();
        query.stream_selector = Some(selector.into());
    }
}

async fn seed_profile_and_channel(fixture: &Fixture, scenario: Scenario) {
    for id in [1, 2] {
        let mut profile = fixture
            .state
            .repositories
            .ffmpeg_profile
            .get(id)
            .await
            .unwrap();
        profile.video_bitrate = 1000;
        profile.video_buffer_size = 2000;
        profile.audio_channels = 1;
        profile.allow_b_frames = false;
        profile.normalize_framerate = id == 2 && scenario.normalize();
        fixture
            .state
            .repositories
            .ffmpeg_profile
            .upsert(&profile)
            .await
            .unwrap();
    }
    let mut channel = fixture
        .state
        .repositories
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    channel.streaming_engine = StreamingEngine::Legacy; // An explicit Next preview must not migrate Native.
    channel.fallback_filler_id = Some(1);
    channel.subtitle_mode = ChannelSubtitleMode::Default;
    channel.preferred_subtitle_language_code = Some("nld".into());
    channel.next_engine_text_subtitle_mode = ersatzrs_domain::NextEngineTextSubtitleMode::Burn;
    fixture
        .state
        .repositories
        .channel
        .upsert(&channel)
        .await
        .unwrap();
}

async fn generate(
    ffmpeg: &FsPath,
    destination: &FsPath,
    color: &str,
    fps: u32,
    dutch: u32,
    opening: bool,
) {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-y", "-f", "lavfi", "-i"])
        .arg(format!("color={color}:s=320x180:r={fps}"))
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(format!("sine=frequency={dutch}:sample_rate=48000"))
        .args([
            "-t",
            "90",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "2",
            "-c:a",
            "aac",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:0",
            "title=English",
            "-metadata:s:a:1",
            "language=nld",
            "-metadata:s:a:1",
            "title=Dutch",
        ]);
    command.arg("-g").arg(fps.to_string());
    if opening {
        command.args(["-vf", "drawbox=color=white:t=fill:enable='lt(t,2.5)'"]);
    }
    command.arg(destination);
    tool(command).await.unwrap();
}

async fn seed_filler(fixture: &Fixture) {
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (12,1,0)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (12)"#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (40,12,'Synthetic fallback',0,'2026-09-26','2026-09-26')"#,
        r#"INSERT INTO "MediaVersion" ("Id","MovieId","Duration","Width","Height","VideoScanKind","RFrameRate","DateAdded","DateUpdated") VALUES (300,12,'00:01:30',320,180,0,'25','2026-09-26','2026-09-26')"#,
        r#"INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId") VALUES (30,0,'h264',1,NULL,0,NULL,1,0,300),(31,1,'aac',2,'eng',1,'English',1,0,300),(32,2,'aac',2,'nld',1,'Dutch',0,0,300)"#,
        r#"INSERT INTO "FillerPreset" ("Id","AllowWatermarks","CollectionType","MediaItemId","FillerKind","FillerMode","UseChaptersAsMediaItems") VALUES (1,0,10,12,4,0,0)"#,
    ] {
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
    }
    sqlx::query(r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (4,300,?,'filler-window')"#)
        .bind(fixture.root.path().join("filler.mkv").to_str().unwrap()).execute(&fixture.state.pool).await.unwrap();
}

async fn timeline_items(fixture: &Fixture, running: bool) {
    put_item(
        fixture,
        41,
        10,
        if running { -3 } else { 3 },
        7,
        Some(PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: None,
        }),
    )
    .await;
    if !running {
        sqlx::query(
            r#"UPDATE "PlayoutItem" SET "InPoint"='00:00:03',"OutPoint"='00:00:07' WHERE "Id"=41"#,
        )
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    }
    put_item(
        fixture,
        42,
        11,
        10,
        25,
        Some(PinnedStreamSelection {
            media_version_id: 200,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: Some(9),
        }),
    )
    .await;
    let path = fixture.root.path().join("second.nl.ass");
    tokio::fs::write(&path, caption("&H00FFFFFF", "0:00:00.00", "0:01:30.00"))
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Subtitle" SET "Codec"='ass',"Path"=? WHERE "Id"=20"#)
        .bind(path.to_str().unwrap())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
}

async fn credit_items(fixture: &Fixture, scenario: Scenario) {
    let pin = scenario.pinned().then_some(PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(2),
        subtitle_stream_index: Some(9),
    });
    put_item(fixture, 42, 10, -3, 12, pin).await;
    put_item(
        fixture,
        43,
        11,
        15,
        25,
        Some(PinnedStreamSelection {
            media_version_id: 200,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: None,
        }),
    )
    .await;
}

async fn put_item(
    fixture: &Fixture,
    id: i64,
    media: i64,
    begin: i64,
    end: i64,
    pin: Option<PinnedStreamSelection>,
) {
    fixture
        .state
        .repositories
        .playout
        .upsert_item(&PlayoutItem {
            id,
            playout_id: 1,
            media_item_id: media,
            start: fixture.at + TimeDelta::seconds(begin),
            finish: fixture.at + TimeDelta::seconds(end),
            in_point: TimeDelta::zero(),
            out_point: TimeDelta::seconds(end - begin),
            pinned_stream_selection: pin,
            ..Default::default()
        })
        .await
        .unwrap();
}

pub(super) async fn remote(fixture: &Fixture, media: i64, version: i64, uri: &str, live: bool) {
    sqlx::query(r#"INSERT INTO "RemoteStream" ("Id","Url","Liveness","IsLive") VALUES (?,?,?,?)"#)
        .bind(media)
        .bind(uri)
        .bind(if live { 2 } else { 1 })
        .bind(i32::from(live))
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    // SupportedMediaVersion deliberately permits a remote backing for a logical movie/music video.
    sqlx::query(r#"UPDATE "MediaVersion" SET "RemoteStreamId"=? WHERE "Id"=?"#)
        .bind(media)
        .bind(version)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (?,?,0,'Synthetic remote','2026-09-26','2026-09-26')"#)
        .bind(1000+media).bind(media).execute(&fixture.state.pool).await.unwrap();
}

async fn music_video(fixture: &Fixture, scenario: Scenario) {
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (31,1,0)"#,
        r#"INSERT INTO "Artist" ("Id") VALUES (31)"#,
        r#"INSERT INTO "ArtistMetadata" ("Id","ArtistId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (32,31,'Window Artist',0,'2026-09-26','2026-09-26')"#,
        r#"INSERT INTO "MusicVideo" ("Id","ArtistId") VALUES (10,31)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"MusicVideoId"=10 WHERE "Id" IN (100,101)"#,
        r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=10"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
        r#"INSERT INTO "MusicVideoMetadata" ("Id","MusicVideoId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (33,10,'Actual live credits',0,'2026-09-26','2026-09-26')"#,
    ] {
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
    }
    let mut channel = fixture
        .state
        .repositories
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    channel.music_video_credits_mode = ChannelMusicVideoCreditsMode::GenerateSubtitles;
    channel.music_video_credits_template = Some("row201-vector-proof".into());
    fixture
        .state
        .repositories
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    let folder = ersatzrs_infra::music_video_credits::music_video_credits_templates_folder();
    tokio::fs::create_dir_all(&folder).await.unwrap();
    let template = format!("{}\n{}", caption("&H000000FF", "0:00:00.00", "0:00:01.00"),
        "Dialogue: 0,0:00:03.00,0:00:06.00,Default,,0,0,0,,{\\an7\\pos(240,120)\\1c&HFFFFFF&\\p1}m 0 0 l 24 0 24 24 0 24");
    tokio::fs::write(folder.join("row201-vector-proof.sbntxt"), template)
        .await
        .unwrap();
    if scenario.pinned() || scenario.custom() {
        let path = fixture.root.path().join("pinned.nl.ass");
        tokio::fs::write(&path, caption("&H00FFFF00", "0:00:00.00", "0:01:30.00"))
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "Subtitle" ("Id","MusicVideoMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (23,33,1,'Owned Dutch',9,'ass',1,0,0,'nld',0,?,'2026-09-26','2026-09-26')"#)
            .bind(path.to_str().unwrap()).execute(&fixture.state.pool).await.unwrap();
    }
}

pub(super) fn caption(color: &str, start: &str, finish: &str) -> String {
    format!("[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,12,{color},{color},&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,{start},{finish},Default,,0,0,0,,{{\\an7\\pos(240,120)\\p1}}m 0 0 l 24 0 24 24 0 24\n")
}

async fn attach_canvas(fixture: &Fixture, item: i64) {
    let config = GraphicsElementConfig::Script(ScriptGraphicsConfig {
        command: Some("return 'setcolor yellow\\nrect 16 16 32 32 fill\\n'".into()),
        ..Default::default()
    });
    fixture
        .state
        .repositories
        .graphics
        .upsert_element(&GraphicsElement {
            id: 901,
            name: "Later occurrence canvas".into(),
            kind: config.kind(),
            config,
            z_index: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId","GraphicsElementId") VALUES (?,901)"#).bind(item).execute(&fixture.state.pool).await.unwrap();
}

async fn attach_source_clock(fixture: &Fixture, item: i64) {
    let font =
        FsPath::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/Roboto-Regular.ttf");
    let text = format!(
        "{{% if Now.Unix < {} %}}X{{% else %}}XX{{% endif %}}",
        (fixture.at + TimeDelta::hours(1) + TimeDelta::seconds(15)).timestamp()
    );
    let config = GraphicsElementConfig::Text(TextGraphicsConfig {
        text: Some(text),
        renderer: TextGraphicsRenderer::Drawtext,
        drawtext: DrawtextGraphicsConfig {
            font_file: Some(font.to_str().unwrap().into()),
            refresh_interval_milliseconds: 100,
            ..Default::default()
        },
        styles: vec![StyleDefinition {
            font_size: Some(18.0),
            text_color: Some("white".into()),
            ..Default::default()
        }],
        ..Default::default()
    });
    fixture
        .state
        .repositories
        .graphics
        .upsert_element(&GraphicsElement {
            id: 902,
            name: "Mirror broadcast-clock proof".into(),
            kind: config.kind(),
            config,
            z_index: 2,
            location: WatermarkLocation::TopLeft,
            horizontal_margin_percent: Some(25.0),
            vertical_margin_percent: Some(100.0 / 3.0),
            ..Default::default()
        })
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId","GraphicsElementId") VALUES (?,902)"#).bind(item).execute(&fixture.state.pool).await.unwrap();
}

pub(super) async fn configure_stop(fixture: &Fixture) {
    put_item(fixture, 42, 10, -3, 60, None).await;
    sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "Id"=43"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
}
