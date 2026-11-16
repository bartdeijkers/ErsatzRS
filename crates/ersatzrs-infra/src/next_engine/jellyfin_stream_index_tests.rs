//! ADR 0137: provider choices stay logical, Next file tracks/hints become physical.
use super::*;
use ersatzrs_domain::NextEngineTextSubtitleMode;

pub(crate) async fn qualify(pool: &DbPool, logical: [i32; 5]) {
    sqlx::raw_sql(r#"
        DELETE FROM "LocalLibrary" WHERE "Id"=1;
        INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1);
        INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (1,1);
        INSERT INTO "JellyfinMovie" ("Id","ItemId") VALUES (10,'synthetic');
        INSERT INTO "JellyfinConnection" ("Id","JellyfinMediaSourceId","Address") VALUES (1,1,'https://jellyfin.example.test');
        UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=1 WHERE "Id"=100;
    "#).execute(pool).await.unwrap();
    for (physical, logical) in logical.into_iter().enumerate() {
        let physical = i32::try_from(physical).unwrap();
        sqlx::query(r#"UPDATE "MediaStream" SET "Index"=? WHERE "Id"=?"#)
            .bind(logical)
            .bind(physical + 1)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "ProviderStreamIndex" ("MediaVersionId","StreamIndex","SourceStreamIndex") VALUES (100,?,?)"#)
            .bind(logical).bind(physical).execute(pool).await.unwrap();
    }
    sqlx::query(r#"UPDATE "Subtitle" SET "StreamIndex"=? WHERE "Id"=1"#)
        .bind(logical[4])
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Subtitle" SET "StreamIndex"=? WHERE "Id"=3"#)
        .bind(logical[3])
        .execute(pool)
        .await
        .unwrap();
}

fn source() -> wire::Source {
    wire::Source::Local {
        path: "media/fixture.mkv".into(),
        in_point_ms: None,
        out_point_ms: None,
        probe_hint: None,
    }
}
fn resolution() -> Resolution {
    Resolution {
        width: 320,
        height: 180,
        ..Default::default()
    }
}

#[tokio::test]
async fn row295_next_pins_route_physical_tracks_and_hints_for_all_external_positions() {
    for logical in [[2, 3, 4, 5, 6], [0, 1, 2, 3, 4], [0, 2, 3, 5, 6]] {
        let pool = fixture_pool().await;
        qualify(&pool, logical).await;
        for mode in [
            NextEngineTextSubtitleMode::Burn,
            NextEngineTextSubtitleMode::Convert,
        ] {
            let mut pinned = item();
            pinned.pinned_stream_selection = Some(PinnedStreamSelection {
                media_version_id: 100,
                video_stream_index: logical[0],
                audio_stream_index: Some(logical[2]),
                subtitle_stream_index: Some(logical[4]),
            });
            let channel = Channel {
                next_engine_text_subtitle_mode: mode,
                ..Default::default()
            };
            let projected =
                project_deferred_item(&pool, &channel, &pinned, 100, source(), &resolution())
                    .await
                    .unwrap();
            let tracks = projected.item.tracks.as_ref().unwrap();
            assert_eq!(tracks.video.as_ref().unwrap().stream_index, Some(0));
            assert_eq!(tracks.audio.as_ref().unwrap().stream_index, Some(2));
            assert_eq!(tracks.subtitle.as_ref().unwrap().stream_index, Some(4));
            assert_eq!(
                projected.selected_subtitle.as_ref().unwrap().stream_index,
                logical[4]
            );
            if let Some(pending) = &projected.pending_burn {
                assert_eq!(pending.subtitle.stream_index, logical[4]);
                assert_eq!(
                    source_indexes::physical(
                        projected.source_indexes.as_ref(),
                        usize::try_from(pending.subtitle.stream_index).unwrap()
                    )
                    .unwrap(),
                    4
                );
            }
            let wire::Source::Local {
                probe_hint: Some(hint),
                ..
            } = &projected.original_source
            else {
                panic!("missing hint")
            };
            assert_eq!(hint.video[0].stream_index, 0);
            assert_eq!(
                hint.audio
                    .iter()
                    .map(|a| a.stream_index)
                    .collect::<Vec<_>>(),
                [1, 2]
            );
            assert_eq!(
                hint.subtitle
                    .iter()
                    .map(|s| s.stream_index)
                    .collect::<Vec<_>>(),
                [3, 4]
            );
            let repeated = project_deferred_item_selected(
                &pool,
                &channel,
                &pinned,
                100,
                source(),
                &resolution(),
                SelectionDirectives {
                    audio: AudioDirective::Published(Some(2)),
                    subtitle_id: None,
                    diagnostics: None,
                    source_pending: false,
                    source_text: None,
                },
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(&repeated.item).unwrap(),
                serde_json::to_value(&projected.item).unwrap(),
                "published physical audio must revalidate against the original logical pin"
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i32>(r#"SELECT "StreamIndex" FROM "Subtitle" WHERE "Id"=2"#)
                .fetch_one(&pool)
                .await
                .unwrap(),
            9,
            "provider sidecar identity is untouched"
        );
    }
}

#[tokio::test]
async fn row295_next_refuses_pending_missing_and_changed_mapping_authority() {
    let pool = fixture_pool().await;
    qualify(&pool, [2, 3, 4, 5, 6]).await;
    let captured = source_indexes::load(&pool, 100).await.unwrap();
    source_indexes::ensure_current(&pool, 100, captured.as_ref())
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaVersion" SET "DateUpdated"='2026-11-16' WHERE "Id"=100"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        source_indexes::ensure_current(&pool, 100, captured.as_ref())
            .await
            .is_err()
    );
    sqlx::query(r#"UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=0 WHERE "Id"=100"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(project_deferred_item(
        &pool,
        &Channel::default(),
        &item(),
        100,
        source(),
        &resolution()
    )
    .await
    .is_err());
    sqlx::query(r#"UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=1 WHERE "Id"=100"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "ProviderStreamIndex" WHERE "StreamIndex"=3"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(project_deferred_item(
        &pool,
        &Channel::default(),
        &item(),
        100,
        source(),
        &resolution()
    )
    .await
    .is_err());
    assert!(super::super::super::probe_hint::persisted(&pool, 100)
        .await
        .is_err());
    // Removing provider ownership must not reinterpret retained logical rows
    // as a local identity map, even if both subtype records disappear.
    sqlx::raw_sql(r#"DELETE FROM "JellyfinMovie"; DELETE FROM "JellyfinLibrary";"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(source_indexes::load(&pool, 100).await.is_err());
}
