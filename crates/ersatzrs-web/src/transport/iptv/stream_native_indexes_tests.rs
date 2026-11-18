//! ADR0137: native transport selection stays logical until final file routing.
use super::tests::playback_profile;
use super::*;
use ersatzrs_domain::HardwareAccelerationKind;
use ersatzrs_infra::playback::NativeStreamIndexMap;

async fn fixture(logical: [i32; 5]) -> DbPool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Synthetic',1,1);
        INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (1,1);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'synthetic');
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (10,1);
        INSERT INTO "Movie" ("Id") VALUES (10);
        INSERT INTO "JellyfinMovie" ("Id","ItemId") VALUES (10,'synthetic');
        INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind","ProviderStreamIndexVersion")
            VALUES (100,10,'2026-11-17','2026-11-17','00:01:00',180,320,0,1);"#).execute(&pool).await.unwrap();
    for (physical, logical) in logical.into_iter().enumerate() {
        let kind = [1, 2, 2, 3, 3][physical];
        sqlx::query(r#"INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Channels","Default","Forced") VALUES (100,?,?,0,0,0)"#)
            .bind(logical).bind(kind).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "ProviderStreamIndex" ("MediaVersionId","StreamIndex","SourceStreamIndex") VALUES (100,?,?)"#)
            .bind(logical).bind(i32::try_from(physical).unwrap()).execute(&pool).await.unwrap();
    }
    pool
}

fn request(
    logical: [i32; 5],
    format: OutputFormat,
    mode: StreamingMode,
) -> (HardwareAccelerationKind, PipelineRequest) {
    let mut profile = playback_profile();
    profile.profile.video_format = FFmpegProfileVideoFormat::Copy;
    profile.profile.audio_format = FFmpegProfileAudioFormat::Copy;
    build_stream_request_with_retained_inputs(
        InputFile::video("synthetic-native.mkv"),
        format,
        Duration::ZERO,
        Some(Duration::from_secs(44)),
        &profile,
        None,
        mode,
        OutputMetadata::default(),
        FfmpegCapabilityReport::default(),
        Some(
            SourceVideoStream::new(ersatzrs_ffmpeg::SourceVideoCodec::H264)
                .with_index(usize::try_from(logical[0]).unwrap()),
        ),
        vec![
            SourceAudioStream::new(ersatzrs_ffmpeg::SourceAudioCodec::Aac)
                .with_index(usize::try_from(logical[1]).unwrap()),
            SourceAudioStream::new(ersatzrs_ffmpeg::SourceAudioCodec::Aac)
                .with_index(usize::try_from(logical[2]).unwrap()),
        ],
        PlaybackAudioSelection::Exact(usize::try_from(logical[2]).unwrap()),
        None,
        Vec::new(),
        Vec::new(),
        None,
        false,
        false,
        None,
    )
}

#[tokio::test]
async fn row295_transport_physical_mapping_preserves_mpegts_and_direct_choices() {
    let mut snapshots = Vec::new();
    for (label, format, mode) in [
        (
            "MPEG-TS",
            OutputFormat::MpegTs,
            StreamingMode::TransportStream,
        ),
        (
            "Direct MKV",
            OutputFormat::Mkv,
            StreamingMode::HttpLiveStreamingDirect,
        ),
        (
            "Direct MP4",
            OutputFormat::Mp4,
            StreamingMode::HttpLiveStreamingDirect,
        ),
    ] {
        let mut reference = None;
        for logical in [[2, 3, 4, 5, 6], [0, 1, 2, 3, 4], [0, 2, 3, 5, 6]] {
            let pool = fixture(logical).await;
            let indexes = NativeStreamIndexMap::capture(&pool, 100).await.unwrap();
            indexes.validate(&pool).await.unwrap();
            let (acceleration, mut request) = request(logical, format, mode);
            let mut caption_request = request.clone();
            caption_request.inputs.push(
                InputFile::image_subtitle(
                    "synthetic-native.mkv",
                    ersatzrs_ffmpeg::SubtitleMethod::Copy,
                )
                .with_stream_index(usize::try_from(logical[4]).unwrap()),
            );
            caption_request.inputs.push(InputFile::text_subtitle(
                "prepared/caption.srt",
                ersatzrs_ffmpeg::SubtitleMethod::Burn,
            ));
            apply_native_indexes(Some(&indexes), "synthetic-native.mkv", &mut caption_request)
                .unwrap();
            assert_eq!(caption_request.inputs[2].streams[0].index, 4);
            assert_eq!(caption_request.inputs[3].streams[0].index, 0);
            apply_native_indexes(Some(&indexes), "synthetic-native.mkv", &mut request).unwrap();
            let command = ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
                acceleration,
                request,
            )
            .unwrap()
            .0;
            if let Some(previous) = &reference {
                assert_eq!(&command.arguments, previous);
            }
            reference = Some(command.arguments);
            sqlx::query(r#"UPDATE "MediaVersion" SET "DateUpdated"='2026-11-18' WHERE "Id"=100"#)
                .execute(&pool)
                .await
                .unwrap();
            assert!(
                indexes.validate(&pool).await.is_err(),
                "retry must reject changed map authority"
            );
            let fresh = NativeStreamIndexMap::capture(&pool, 100).await.unwrap();
            sqlx::raw_sql(
                r#"INSERT INTO "MediaSource" ("Id") VALUES (2);
                INSERT INTO "JellyfinMediaSource" ("Id") VALUES (2);
                UPDATE "Library" SET "MediaSourceId"=2 WHERE "Id"=1;"#,
            )
            .execute(&pool)
            .await
            .unwrap();
            assert!(
                fresh.validate(&pool).await.is_err(),
                "moving the same library to another server invalidates native authority"
            );
        }
        snapshots.push(format!("{label}:\n{}", reference.unwrap().join("\n")));
    }
    insta::assert_snapshot!(
        "row295_native_transport_physical_tracks",
        snapshots.join("\n\n")
    );
}
