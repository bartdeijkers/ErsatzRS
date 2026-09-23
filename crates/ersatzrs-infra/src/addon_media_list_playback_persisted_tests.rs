use super::*;

#[tokio::test]
async fn persisted_resolution_keeps_prepared_publication_and_never_falls_back() {
    let (root, pool) = super::tests::fixture().await;
    let runtime = AddonRuntimeContext::default();
    // Prepared bytes survive disabled/missing provider access and unclassified
    // mutable source metadata. The publication remains authoritative.
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled"=0"#)
        .execute(&pool)
        .await
        .unwrap();
    let stream = resolve_persisted_remote_stream(&pool, 100, &runtime, root.path())
        .await
        .unwrap();
    assert!(matches!(stream.source, RemoteStreamSource::Prepared(_)));
    assert_eq!(stream.liveness, RemoteStreamLiveness::Finite);
    tokio::fs::remove_file(root.path().join("objects/prepared.media"))
        .await
        .unwrap();
    assert!(
        resolve_persisted_remote_stream(&pool, 100, &runtime, root.path())
            .await
            .is_err()
    );
    // Make a direct fallback viable while the published ownership is invalid.
    // The managed binding must still fail instead of bypassing publication.
    sqlx::query(r#"UPDATE "RemoteStream" SET "AddonId"=NULL, "Liveness"=1, "IsLive"=0, "Duration"='00:00:12.500'"#)
        .execute(&pool).await.unwrap();
    let direct = RemoteStreamDefinition {
        url: Some("https://example.test/item".into()),
        duration: Some("00:00:12.500".into()),
        liveness: Some(RemoteStreamLiveness::Finite),
        ..Default::default()
    };
    assert!(
        crate::remote_stream::resolve_remote_stream_with_addons(&pool, &direct, &runtime)
            .await
            .is_ok()
    );
    assert!(
        resolve_persisted_remote_stream(&pool, 100, &runtime, root.path())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn persisted_direct_resolution_preserves_liveness_and_rejects_legacy_scripts() {
    let (root, pool) = super::tests::fixture().await;
    let runtime = AddonRuntimeContext::default();
    // Turn this disposable fixture into an unmanaged direct Remote Stream.
    sqlx::query(r#"DELETE FROM "AddonMediaListLibrary""#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "RemoteStream" SET "AddonId"=NULL, "Duration"='00:00:12.500', "Liveness"=1, "IsLive"=0"#)
        .execute(&pool).await.unwrap();
    let finite = resolve_persisted_remote_stream(&pool, 100, &runtime, root.path())
        .await
        .unwrap();
    assert!(matches!(finite.source, RemoteStreamSource::Url(_)));
    assert_eq!(finite.configured_duration.as_deref(), Some("00:00:12.500"));
    assert_eq!(finite.liveness, RemoteStreamLiveness::Finite);
    for (url, liveness, legacy_live, allowed) in [
        ("https://example.test/live", 2, 1, true),
        ("rtsp://example.test/live", 2, 1, true),
        ("https://example.test/unknown", 0, 0, false),
        ("https://example.test/inconsistent", 2, 0, false),
    ] {
        sqlx::query(
            r#"UPDATE "RemoteStream" SET "Url"=?, "Liveness"=?, "IsLive"=?, "Duration"=NULL"#,
        )
        .bind(url)
        .bind(liveness)
        .bind(legacy_live)
        .execute(&pool)
        .await
        .unwrap();
        let resolved = resolve_persisted_remote_stream(&pool, 100, &runtime, root.path()).await;
        assert_eq!(resolved.is_ok(), allowed);
        if let Ok(stream) = resolved {
            assert_eq!(stream.liveness, RemoteStreamLiveness::Live);
            assert!(stream.is_live);
        }
    }
    sqlx::query(r#"UPDATE "RemoteStream" SET "Script"='fixture-legacy-command'"#)
        .execute(&pool)
        .await
        .unwrap();
    let error = resolve_persisted_remote_stream(&pool, 100, &runtime, root.path())
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("migration to an add-on"));
    assert!(!error.to_string().contains("fixture-legacy-command"));
    assert!(
        resolve_persisted_remote_stream(&pool, 999, &runtime, root.path())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn persisted_installed_addon_never_degrades_to_its_direct_url() {
    let (root, pool) = super::tests::fixture().await;
    sqlx::query(r#"DELETE FROM "AddonMediaListLibrary""#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "RemoteStream" SET "Liveness"=2, "IsLive"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled"=0"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(resolve_persisted_remote_stream(
        &pool,
        100,
        &AddonRuntimeContext::default(),
        root.path()
    )
    .await
    .is_err());
}
