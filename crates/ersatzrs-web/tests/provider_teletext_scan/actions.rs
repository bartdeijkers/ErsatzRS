//! Native qualification of the remaining managed provider report actions.
use super::*;
use ersatzrs_core::repositories::ManagedLibraryOperationClaim;

async fn scan(
    pool: &SqlitePool,
    provider: MediaSourceKind,
    show_id: Option<i64>,
    claim: Option<&ManagedLibraryOperationClaim>,
    server: fixture::Provider,
) -> Result<(), ersatzrs_app::ScanRuntimeError> {
    let runtime = NativeScanRuntime::new(pool.clone());
    let cleanup = LocalProbeCleanupScope::new();
    let result = cleanup
        .scope(async {
            if let Some(show_id) = show_id {
                runtime
                    .scan_show(provider, 1, show_id, true, &(), claim)
                    .await
            } else {
                runtime
                    .scan_library(provider, 1, true, &(), claim)
                    .await
                    .map(|_| ())
            }
        })
        .await;
    cleanup.close();
    cleanup.wait_cleanup().await.unwrap();
    assert_eq!(cleanup.cleanup_status(), Ok(true));
    let counts = server.close().await;
    let kind: i64 = sqlx::query_scalar(r#"SELECT "MediaKind" FROM "Library" WHERE "Id"=1"#)
        .fetch_one(pool)
        .await
        .unwrap();
    let expected = match (provider, kind, show_id.is_some()) {
        (MediaSourceKind::Plex, 2, true) => [3, 0],
        (MediaSourceKind::Plex, 2, false) => [4, 0],
        (MediaSourceKind::Plex, 4, _) => [2, 0],
        (MediaSourceKind::Plex, _, _) => [1, 0],
        (_, 2, _) => [3, 1],
        _ => [1, 1],
    };
    assert_eq!(
        counts, expected,
        "actual metadata and selected playback routes must be consumed"
    );
    result
}

async fn report_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn advance_revision(local: &Path) {
    let changed =
        std::fs::metadata(local).unwrap().modified().unwrap() + std::time::Duration::from_mins(2);
    std::fs::File::options()
        .write(true)
        .open(local)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(changed))
        .unwrap();
}

#[tokio::test]
#[ignore = "requires FFprobe9 and ERSATZRS_TELETEXT_SCAN_FIXTURE synthetic AV/200/888"]
async fn managed_mapped_provider_movie_and_other_video_reports_preserve_tracks() {
    let (probe, input) = inputs();
    let root = tempfile::tempdir().unwrap();
    let local = root.path().join("movie.ts");
    std::fs::copy(input, &local).unwrap();
    let source = Box::pin(qualified_source(&probe, &local)).await;
    for (provider, other_video) in [
        (MediaSourceKind::Emby, false),
        (MediaSourceKind::Plex, false),
        (MediaSourceKind::Plex, true),
    ] {
        let server =
            fixture::Provider::for_action(&source, provider, false, fixture::Declaration::Exact)
                .await;
        let pool = fixture::database_for(root.path(), &server.address, provider, false).await;
        if other_video {
            sqlx::query(r#"UPDATE "Library" SET "MediaKind"=4 WHERE "Id"=1"#)
                .execute(&pool)
                .await
                .unwrap();
        }
        let claim = claim_for(&pool, provider, None).await;
        Box::pin(scan(&pool, provider, None, Some(&claim), server))
            .await
            .unwrap();
        let ids = Box::pin(assert_ordinary(&pool, &source)).await;
        Box::pin(assert_report_for(&pool, &source, ids, provider)).await;
        assert_eq!(report_count(&pool).await, 1);
        let owner: (Option<i64>, Option<i64>) =
            sqlx::query_as(r#"SELECT "MovieId","OtherVideoId" FROM "MediaVersion" WHERE "Id"=?"#)
                .bind(ids.1)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(owner.0.is_some(), !other_video);
        assert_eq!(owner.1.is_some(), other_video);
        pool.close().await;
    }
}

#[tokio::test]
#[ignore = "requires FFprobe9 and ERSATZRS_TELETEXT_SCAN_FIXTURE synthetic AV/200/888"]
async fn managed_mapped_provider_episode_and_targeted_show_refresh_exact_report() {
    let (probe, input) = inputs();
    let root = tempfile::tempdir().unwrap();
    let local = root.path().join("movie.ts");
    std::fs::copy(input, &local).unwrap();
    for provider in [
        MediaSourceKind::Jellyfin,
        MediaSourceKind::Emby,
        MediaSourceKind::Plex,
    ] {
        let source = Box::pin(qualified_source(&probe, &local)).await;
        let server =
            fixture::Provider::for_action(&source, provider, true, fixture::Declaration::Exact)
                .await;
        let pool = fixture::database_for(root.path(), &server.address, provider, true).await;
        let initial = claim_for(&pool, provider, None).await;
        Box::pin(scan(&pool, provider, None, Some(&initial), server))
            .await
            .unwrap();
        let ids = Box::pin(assert_ordinary(&pool, &source)).await;
        Box::pin(assert_report_for(&pool, &source, ids, provider)).await;
        let show_id: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "Show""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(SqliteManagedLibraryOperationRepository(pool.clone())
            .complete_operation(&initial)
            .await
            .unwrap());
        advance_revision(&local);
        let refreshed = Box::pin(qualified_source(&probe, &local)).await;
        assert_ne!(
            source.teletext.as_ref().unwrap().revision(),
            refreshed.teletext.as_ref().unwrap().revision()
        );
        let server =
            fixture::Provider::for_action(&refreshed, provider, true, fixture::Declaration::Exact)
                .await;
        fixture::update_connection(&pool, &server.address, provider).await;
        let targeted = claim_for(&pool, provider, Some(show_id)).await;
        assert_ne!(targeted.operation.id, initial.operation.id);
        assert_eq!(targeted.operation.scan.unwrap().show_id, Some(show_id));
        Box::pin(scan(
            &pool,
            provider,
            Some(show_id),
            Some(&targeted),
            server,
        ))
        .await
        .unwrap();
        let new_ids = Box::pin(assert_ordinary(&pool, &refreshed)).await;
        assert_eq!(
            (ids.0, ids.1),
            (new_ids.0, new_ids.1),
            "targeted deep rescan preserves file/version identity"
        );
        Box::pin(assert_report_for(&pool, &refreshed, new_ids, provider)).await;
        assert_eq!(
            report_count(&pool).await,
            1,
            "only the refreshed source binding survives"
        );
        pool.close().await;
    }
}

#[tokio::test]
#[ignore = "requires FFprobe9 and ERSATZRS_TELETEXT_SCAN_FIXTURE synthetic AV/200/888"]
async fn managed_mapped_provider_refusals_preserve_ordinary_tracks_and_lease_fence() {
    let (probe, input) = inputs();
    let root = tempfile::tempdir().unwrap();
    let local = root.path().join("movie.ts");
    std::fs::copy(input, &local).unwrap();
    let source = Box::pin(qualified_source(&probe, &local)).await;
    for provider in [MediaSourceKind::Emby, MediaSourceKind::Plex] {
        // Plex's selected Part.file is also the item path: its path mismatch is
        // not an independent declaration/item witness. Its native index must
        // nevertheless agree, just like Jellyfin and Emby.
        let declarations = if provider == MediaSourceKind::Emby {
            vec![
                fixture::Declaration::SelectedPathMismatch,
                fixture::Declaration::NativeIndexMismatch,
            ]
        } else {
            vec![fixture::Declaration::NativeIndexMismatch]
        };
        for declaration in declarations {
            let index = if matches!(declaration, fixture::Declaration::NativeIndexMismatch) {
                4
            } else {
                3
            };
            let server = fixture::Provider::for_action(&source, provider, false, declaration).await;
            let pool = fixture::database_for(root.path(), &server.address, provider, false).await;
            let claim = claim_for(&pool, provider, None).await;
            Box::pin(scan(&pool, provider, None, Some(&claim), server))
                .await
                .unwrap();
            Box::pin(assert_ordinary_at(&pool, &source, index)).await;
            assert_eq!(
                report_count(&pool).await,
                0,
                "a mismatched declaration/native layout has no binding"
            );
            pool.close().await;
        }
        let server =
            fixture::Provider::for_action(&source, provider, false, fixture::Declaration::Exact)
                .await;
        let pool = fixture::database_for(root.path(), &server.address, provider, false).await;
        let claim = claim_for(&pool, provider, None).await;
        let changed = sqlx::query(r#"UPDATE "ManagedLibraryLease" SET "LeaseExpiresAt"='2000-01-01 00:00:00' WHERE "OperationId"=?"#)
            .bind(claim.operation.id).execute(&pool).await.unwrap();
        assert_eq!(changed.rows_affected(), 1);
        let error = Box::pin(scan(&pool, provider, None, Some(&claim), server))
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("mapped source scan claim is no longer current"));
        for (table, query) in [
            ("Movie", r#"SELECT COUNT(*) FROM "Movie""#),
            ("MovieMetadata", r#"SELECT COUNT(*) FROM "MovieMetadata""#),
            ("MediaItem", r#"SELECT COUNT(*) FROM "MediaItem""#),
            ("MediaVersion", r#"SELECT COUNT(*) FROM "MediaVersion""#),
            ("MediaFile", r#"SELECT COUNT(*) FROM "MediaFile""#),
            ("MediaStream", r#"SELECT COUNT(*) FROM "MediaStream""#),
            (
                "SourceTeletextMetadata",
                r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#,
            ),
        ] {
            let count: i64 = sqlx::query_scalar(query).fetch_one(&pool).await.unwrap();
            assert_eq!(count, 0, "expired {provider:?} claim wrote {table}");
        }
        pool.close().await;
    }
}
