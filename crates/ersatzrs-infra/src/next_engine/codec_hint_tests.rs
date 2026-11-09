use super::*;

#[tokio::test]
async fn unnamed_persisted_codecs_preserve_complete_hint_and_named_tracks() {
    let pool = super::super::selection::tests::fixture_pool().await;
    sqlx::query(r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (1,100,'synthetic.mkv','synthetic')"#)
        .execute(&pool).await.unwrap();
    let original = persisted(&pool, 100).await.unwrap().unwrap();
    let original = serde_json::to_value(&original).unwrap();
    assert_eq!(original["format_name"], "matroska,webm");
    for (kind, key) in [(1, "video"), (2, "audio"), (3, "subtitle")] {
        for codec in [None, Some(""), Some(" \t\r\n")] {
            sqlx::query(r#"UPDATE "MediaStream" SET "Codec"=? WHERE "MediaVersionId"=100 AND "MediaStreamKind"=?"#)
                .bind(codec).bind(kind).execute(&pool).await.unwrap();
            let hint = serde_json::to_value(persisted(&pool, 100).await.unwrap().unwrap()).unwrap();
            let mut expected = original.clone();
            for stream in expected[key].as_array_mut().unwrap() {
                stream["codec"] = "unknown".into();
            }
            assert_eq!(hint, expected, "kind={kind}, codec={codec:?}");
        }
        // Restore the exact named codecs (including the two distinct subtitles)
        // before exercising another kind; all unedited fields remain identical.
        for stream in original[key].as_array().unwrap() {
            sqlx::query(
                r#"UPDATE "MediaStream" SET "Codec"=? WHERE "MediaVersionId"=100 AND "Index"=?"#,
            )
            .bind(stream["codec"].as_str().unwrap())
            .bind(stream["stream_index"].as_i64().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    assert_eq!(
        serde_json::to_value(persisted(&pool, 100).await.unwrap().unwrap()).unwrap(),
        original
    );
}

#[tokio::test]
async fn unnamed_video_keeps_next_selection_and_metadata_without_changing_native_policy() {
    use crate::next_engine::selection::source_video;
    use crate::playback::load_source_video_stream_by_index;
    use ersatzrs_ffmpeg::SourceVideoCodec;
    let pool = super::super::selection::tests::fixture_pool().await;
    let mut expected = source_video(&pool, 100, None).await.unwrap().unwrap();
    expected.codec = SourceVideoCodec::Unknown;
    for codec in [None, Some(""), Some(" \t")] {
        sqlx::query(
            r#"UPDATE "MediaStream" SET "Codec"=? WHERE "MediaVersionId"=100 AND "Index"=0"#,
        )
        .bind(codec)
        .execute(&pool)
        .await
        .unwrap();
        for pin in [None, Some(0)] {
            assert_eq!(
                source_video(&pool, 100, pin).await.unwrap(),
                Some(expected.clone())
            );
        }
        assert!(source_video(&pool, 100, Some(99)).await.is_err());
        if codec.is_none() {
            assert!(load_source_video_stream_by_index(&pool, 100, None)
                .await
                .unwrap()
                .is_none());
            assert!(load_source_video_stream_by_index(&pool, 100, Some(0))
                .await
                .is_err());
        }
    }
    sqlx::query(
        r#"UPDATE "MediaStream" SET "AttachedPic"=1 WHERE "MediaVersionId"=100 AND "Index"=0"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(source_video(&pool, 100, None).await.unwrap().is_none());
    assert!(source_video(&pool, 100, Some(0)).await.is_err());
}
