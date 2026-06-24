#![cfg(not(feature = "mysql"))]
mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{
    ChannelEditorForm, ChannelLogoForm, ChannelNumberUpdate, ChannelRepository,
    FfmpegProfileRepository,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, Row};

#[tokio::test]
async fn channel_repository_round_trips_and_filters_enabled_channels() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "default"))
        .await
        .unwrap();

    let mut enabled = common::channel(10, "10.1", true);
    enabled.artwork_ids = vec![100, 101];
    let disabled = common::channel(11, "11", false);

    pool.execute(
        r#"INSERT INTO "Artwork" ("Id", "Path", "ArtworkKind", "DateAdded", "DateUpdated") VALUES
         (100, '/art/100.png', 'Poster', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'),
         (101, '/art/101.png', 'Poster', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')"#,
    )
    .await
    .unwrap();

    repos.channel.upsert(&enabled).await.unwrap();
    repos.channel.upsert(&disabled).await.unwrap();

    let loaded = repos.channel.get_with_profile(10).await.unwrap();
    assert_eq!(loaded.number, "10.1");
    assert_eq!(loaded.artwork_ids, vec![100, 101]);

    let by_number = repos.channel.get_by_number("10.1").await.unwrap();
    assert_eq!(by_number.id, 10);

    let enabled_channels = repos.channel.list_enabled().await.unwrap();
    assert_eq!(enabled_channels.len(), 1);
    assert_eq!(enabled_channels[0].id, 10);
    assert!(enabled_channels[0].artwork_ids.is_empty());
}

#[tokio::test]
async fn channel_editor_save_round_trips_full_form_and_logo_artwork() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_editor_dependencies(&pool, &repos).await;

    let result = repos
        .channel
        .save_editor_form(ChannelEditorForm {
            number: "42.1".to_owned(),
            name: "  Full Channel  ".to_owned(),
            group: "  Prime  ".to_owned(),
            categories: Some("Movies,TV".to_owned()),
            ffmpeg_profile_id: 1,
            slug_seconds: Some(2.5),
            logo: ChannelLogoForm {
                path: Some("iptv/logos/custom.png".to_owned()),
                content_type: Some("image/png".to_owned()),
            },
            stream_selector_mode: 1,
            stream_selector: Some("selector.lua".to_owned()),
            preferred_audio_language_code: Some("eng".to_owned()),
            preferred_audio_title: Some("Commentary".to_owned()),
            playout_source: 0,
            playout_mode: 1,
            streaming_mode: 5,
            watermark_id: Some(7),
            fallback_filler_id: Some(9),
            preferred_subtitle_language_code: Some("nld".to_owned()),
            subtitle_mode: 3,
            music_video_credits_mode: 1,
            music_video_credits_template: Some("credits.svg".to_owned()),
            song_video_mode: 1,
            transcode_mode: 0,
            idle_behavior: 1,
            is_enabled: true,
            show_in_epg: true,
            ..ChannelEditorForm::default()
        })
        .await
        .unwrap();

    assert!(result.is_success(), "{:?}", result.validation_errors);
    let id = result.id;

    let state = repos
        .channel
        .get_editor_state(Some(id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.form.number, "42.1");
    assert_eq!(state.form.name, "Full Channel");
    assert_eq!(state.form.group, "Prime");
    assert_eq!(state.form.slug_seconds, Some(2.5));
    assert_eq!(
        state.form.logo.path.as_deref(),
        Some("iptv/logos/custom.png")
    );
    assert_eq!(state.form.logo.content_type.as_deref(), Some("image/png"));
    assert_eq!(state.form.watermark_id, Some(7));
    assert_eq!(state.form.fallback_filler_id, Some(9));
    assert_eq!(
        state.form.music_video_credits_template.as_deref(),
        Some("credits.svg")
    );

    let row = sqlx::query(
        r#"SELECT c."SortNumber", c."ShowInEpg", a."Path", a."OriginalContentType"
           FROM "Channel" c
           JOIN "Artwork" a ON a."ChannelId" = c."Id" AND a."ArtworkKind" = 2
           WHERE c."Id" = ?"#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let sort_number = row.try_get::<f64, _>("SortNumber").unwrap();
    assert!((sort_number - 42.1).abs() < f64::EPSILON);
    assert_eq!(row.try_get::<i64, _>("ShowInEpg").unwrap(), 1);
    assert_eq!(row.try_get::<String, _>("Path").unwrap(), "custom.png");
    assert_eq!(
        row.try_get::<String, _>("OriginalContentType").unwrap(),
        "image/png"
    );
}

#[tokio::test]
async fn channel_editor_language_options_match_ersatztv_media_codes() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_editor_dependencies(&pool, &repos).await;

    let mut channel = editor_form("43", "Language Seed");
    channel.preferred_audio_language_code = Some("zzx".to_owned());
    let result = repos.channel.save_editor_form(channel).await.unwrap();
    assert!(result.is_success(), "{:?}", result.validation_errors);

    pool.execute(
        r#"INSERT INTO "MediaVersion" (
            "Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind"
           ) VALUES (900, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', '00:01:00', 720, 1280, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaStream" (
            "Id", "Index", "MediaStreamKind", "Language", "Channels",
            "Default", "Forced", "MediaVersionId"
           ) VALUES (901, 0, 2, 'eng', 2, 1, 0, 900)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Subtitle" (
            "Id", "DateAdded", "DateUpdated", "Default", "Forced",
            "Language", "StreamIndex", "SubtitleKind"
           ) VALUES (902, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 0, 0, 'spa', 2, 0)"#,
    )
    .await
    .unwrap();

    let state = repos
        .channel
        .get_editor_state(Some(result.id))
        .await
        .unwrap()
        .unwrap();
    let language_options = state
        .options
        .language_codes
        .iter()
        .map(|option| {
            (
                option.value.as_deref().unwrap_or_default(),
                option.label.as_str(),
            )
        })
        .collect::<Vec<_>>();

    assert!(language_options.contains(&("eng", "English")));
    assert!(language_options.contains(&("spa", "Spanish")));
    assert!(language_options.contains(&("zzx", "zzx")));
}

#[tokio::test]
async fn channel_editor_lists_bundled_music_video_credit_templates() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_editor_dependencies(&pool, &repos).await;

    let state = repos.channel.get_editor_state(None).await.unwrap().unwrap();

    assert_eq!(
        state.options.music_video_credit_templates,
        vec![
            "_ArtistTitleAlbum_CenterTop".to_owned(),
            "_ArtistTitle_LeftMiddle".to_owned(),
            "_default.ass".to_owned(),
        ]
    );
}

#[tokio::test]
async fn channel_editor_save_validates_parity_rules() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_editor_dependencies(&pool, &repos).await;

    let result = repos
        .channel
        .save_editor_form(editor_form("50", "Existing"))
        .await
        .unwrap();
    assert!(result.is_success(), "{:?}", result.validation_errors);

    let duplicate = repos
        .channel
        .save_editor_form(editor_form("50", "Duplicate"))
        .await
        .unwrap();
    assert_has_error(&duplicate.validation_errors, "number");

    let mut missing = editor_form("50.1", "Missing");
    missing.id = 9999;
    let missing = repos.channel.save_editor_form(missing).await.unwrap();
    assert_has_error(&missing.validation_errors, "id");

    let mut invalid = editor_form("51", "Invalid");
    invalid.is_enabled = false;
    invalid.show_in_epg = true;
    invalid.fallback_filler_id = Some(10);
    invalid.external_logo_url = Some("ftp://example.invalid/logo.png".to_owned());
    let invalid = repos.channel.save_editor_form(invalid).await.unwrap();
    assert_has_error(&invalid.validation_errors, "show_in_epg");
    assert_has_error(&invalid.validation_errors, "fallback_filler_id");
    assert_has_error(&invalid.validation_errors, "external_logo_url");

    let mut localhost_logo = editor_form("51.1", "Localhost Logo");
    localhost_logo.external_logo_url = Some("http://localhost/logo.png".to_owned());
    let localhost_logo = repos
        .channel
        .save_editor_form(localhost_logo)
        .await
        .unwrap();
    assert!(
        localhost_logo.is_success(),
        "{:?}",
        localhost_logo.validation_errors
    );

    let mut mirror = editor_form("52", "Mirror");
    mirror.playout_source = 1;
    mirror.mirror_source_channel_id = Some(result.id);
    mirror.playout_offset = Some(Duration::hours(13));
    let mirror = repos.channel.save_editor_form(mirror).await.unwrap();
    assert_has_error(&mirror.validation_errors, "playout_offset");
}

#[tokio::test]
async fn channel_number_editor_lists_and_swaps_numbers_atomically() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_editor_dependencies(&pool, &repos).await;

    let first_id = repos
        .channel
        .save_editor_form(editor_form("1", "First"))
        .await
        .unwrap()
        .id;
    let second_id = repos
        .channel
        .save_editor_form(editor_form("2", "Second"))
        .await
        .unwrap()
        .id;
    let tenth_id = repos
        .channel
        .save_editor_form(editor_form("10", "Tenth"))
        .await
        .unwrap()
        .id;

    let rows = repos.channel.list_number_rows().await.unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (row.id, row.number.as_str(), row.name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (first_id, "1", "First"),
            (second_id, "2", "Second"),
            (tenth_id, "10", "Tenth"),
        ]
    );

    let result = repos
        .channel
        .save_number_updates(vec![
            ChannelNumberUpdate {
                id: first_id,
                number: "2".to_owned(),
            },
            ChannelNumberUpdate {
                id: second_id,
                number: "1".to_owned(),
            },
            ChannelNumberUpdate {
                id: tenth_id,
                number: "10".to_owned(),
            },
        ])
        .await
        .unwrap();
    assert!(result.is_success(), "{:?}", result.validation_errors);
    assert_eq!(result.changed_ids, vec![first_id, second_id]);

    let rows = repos.channel.list_number_rows().await.unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (row.id, row.number.as_str()))
            .collect::<Vec<_>>(),
        vec![(second_id, "1"), (first_id, "2"), (tenth_id, "10")]
    );

    let saved = sqlx::query(
        r#"SELECT "Id", "Number", "SortNumber"
           FROM "Channel"
           WHERE "Id" IN (?, ?)
           ORDER BY "Id""#,
    )
    .bind(first_id)
    .bind(second_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(saved[0].try_get::<String, _>("Number").unwrap(), "2");
    assert!((saved[0].try_get::<f64, _>("SortNumber").unwrap() - 2.0).abs() < f64::EPSILON);
    assert_eq!(saved[1].try_get::<String, _>("Number").unwrap(), "1");
    assert!((saved[1].try_get::<f64, _>("SortNumber").unwrap() - 1.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn channel_number_editor_rejects_invalid_and_duplicate_numbers() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_editor_dependencies(&pool, &repos).await;

    let first_id = repos
        .channel
        .save_editor_form(editor_form("3", "First"))
        .await
        .unwrap()
        .id;
    let second_id = repos
        .channel
        .save_editor_form(editor_form("4", "Second"))
        .await
        .unwrap()
        .id;

    let invalid = repos
        .channel
        .save_number_updates(vec![
            ChannelNumberUpdate {
                id: first_id,
                number: "abc".to_owned(),
            },
            ChannelNumberUpdate {
                id: second_id,
                number: "4".to_owned(),
            },
        ])
        .await
        .unwrap();
    assert_has_error(&invalid.validation_errors, &format!("number_{first_id}"));

    let duplicate = repos
        .channel
        .save_number_updates(vec![
            ChannelNumberUpdate {
                id: first_id,
                number: "5".to_owned(),
            },
            ChannelNumberUpdate {
                id: second_id,
                number: "5".to_owned(),
            },
        ])
        .await
        .unwrap();
    assert_has_error(&duplicate.validation_errors, &format!("number_{second_id}"));

    let rows = repos.channel.list_number_rows().await.unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row.number.as_str())
            .collect::<Vec<_>>(),
        vec!["3", "4"]
    );
}

#[tokio::test]
async fn channel_editor_save_replaces_logo_and_preserves_generated_source_with_playouts() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_editor_dependencies(&pool, &repos).await;

    let mut source_form = editor_form("60", "Source");
    source_form.playout_source = 0;
    let source_id = repos
        .channel
        .save_editor_form(source_form)
        .await
        .unwrap()
        .id;

    let mut target_form = editor_form("61", "Target");
    target_form.logo = ChannelLogoForm {
        path: Some("iptv/logos/original.png".to_owned()),
        content_type: Some("image/png".to_owned()),
    };
    let target_id = repos
        .channel
        .save_editor_form(target_form)
        .await
        .unwrap()
        .id;

    sqlx::query(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
           VALUES (600, ?, 0, 1)"#,
    )
    .bind(target_id)
    .execute(&pool)
    .await
    .unwrap();

    let mut update = editor_form("61", "Target Updated");
    update.id = target_id;
    update.playout_source = 1;
    update.mirror_source_channel_id = Some(source_id);
    update.playout_offset = Some(Duration::hours(-1));
    update.external_logo_url = Some("https://example.com/logo.png".to_owned());
    update.logo = ChannelLogoForm::default();
    let update = repos.channel.save_editor_form(update).await.unwrap();
    assert!(update.is_success(), "{:?}", update.validation_errors);

    let row = sqlx::query(
        r#"SELECT c."PlayoutSource", c."MirrorSourceChannelId", c."PlayoutOffset",
                  a."Path", a."OriginalContentType"
           FROM "Channel" c
           JOIN "Artwork" a ON a."ChannelId" = c."Id" AND a."ArtworkKind" = 2
           WHERE c."Id" = ?"#,
    )
    .bind(target_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.try_get::<i64, _>("PlayoutSource").unwrap(), 0);
    assert_eq!(
        row.try_get::<Option<i64>, _>("MirrorSourceChannelId")
            .unwrap(),
        None
    );
    assert_eq!(
        row.try_get::<Option<String>, _>("PlayoutOffset").unwrap(),
        None
    );
    assert_eq!(
        row.try_get::<String, _>("Path").unwrap(),
        "https://example.com/logo.png"
    );
    assert_eq!(
        row.try_get::<Option<String>, _>("OriginalContentType")
            .unwrap(),
        None
    );

    let artwork_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Artwork" WHERE "ChannelId" = ? AND "ArtworkKind" = 2"#,
    )
    .bind(target_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(artwork_count, 1);
}

fn editor_form(number: &str, name: &str) -> ChannelEditorForm {
    ChannelEditorForm {
        number: number.to_owned(),
        name: name.to_owned(),
        group: "ErsatzTV".to_owned(),
        ffmpeg_profile_id: 1,
        streaming_mode: 5,
        playout_source: 0,
        playout_mode: 0,
        stream_selector_mode: 0,
        music_video_credits_mode: 0,
        song_video_mode: 0,
        transcode_mode: 0,
        idle_behavior: 0,
        is_enabled: true,
        show_in_epg: true,
        ..ChannelEditorForm::default()
    }
}

async fn seed_editor_dependencies(
    pool: &sqlx::SqlitePool,
    repos: &ersatzrs_infra::repositories::Repositories,
) {
    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "default"))
        .await
        .unwrap();

    pool.execute(
        r#"INSERT INTO "ChannelWatermark" (
            "Id", "Name", "DurationSeconds", "FrequencyMinutes",
            "HorizontalMarginPercent", "ImageSource", "Location", "Mode",
            "Opacity", "PlaceWithinSourceContent", "Size",
            "VerticalMarginPercent", "WidthPercent", "ZIndex"
           ) VALUES (7, 'Bug', 0, 0, 0, 0, 0, 0, 100, 0, 0, 0, 10, 0)"#,
    )
    .await
    .unwrap();

    pool.execute(
        r#"INSERT INTO "FillerPreset" (
            "Id", "Name", "AllowWatermarks", "CollectionType", "FillerKind",
            "FillerMode", "UseChaptersAsMediaItems"
           ) VALUES
           (9, 'Fallback', 1, 0, 5, 0, 0),
           (10, 'Non Fallback', 1, 0, 1, 0, 0)"#,
    )
    .await
    .unwrap();
}

fn assert_has_error(errors: &[ersatzrs_core::repositories::ChannelValidationError], field: &str) {
    assert!(
        errors.iter().any(|error| error.field == field),
        "expected validation error for {field}, got {errors:?}"
    );
}
