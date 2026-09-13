mod common;

#[path = "addon_media_list_repository/editor.rs"]
mod editor;

use ersatzrs_core::repositories::{
    AddonMediaListAdminSortDirection, AddonMediaListAdminSortField, AddonMediaListItemReviewEdit,
    AddonMediaListNfoMetadata, AddonMediaListPublicationBlockerKind, AddonMediaListRepository,
    RepositoryError,
};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListPresentationKind, AddonMediaListRefreshCadence,
    RemoteStreamLiveness,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn addon_media_list_repository_lists_strict_rows_and_persists_page_size() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
            "AutoRefresh", "RefreshCadence", "GeneratePlaylist", "ItemCount"
           ) VALUES (
            1, 'org.example.lists', 'alice/watchlist',
            'https://example.invalid/alice/watchlist', 'Watchlist', 1, 0, 0, 2
           )"#,
    )
    .await
    .unwrap();

    pool.execute(
        r#"INSERT INTO "AddonMediaListItem" (
            "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
            "Rank", "Title", "MediaItemId"
           ) VALUES (10, 1, 'movie:100', 'Movie', 0, 1, 'Movie', NULL)"#,
    )
    .await
    .unwrap();

    repositories
        .addon_media_lists
        .set_page_size(25)
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .get_page_size()
            .await
            .unwrap(),
        25
    );

    let (rows, total) = repositories
        .addon_media_lists
        .list_admin(
            0,
            25,
            AddonMediaListAdminSortField::Name,
            AddonMediaListAdminSortDirection::Ascending,
        )
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].addon_id, "org.example.lists");
    assert_eq!(rows[0].provider_id, "alice/watchlist");
    assert_eq!(rows[0].match_count, 0);
    assert_eq!(rows[0].refresh_cadence, AddonMediaListRefreshCadence::Daily);
    assert!(!rows[0].generate_playlist);
    assert_eq!(
        rows[0].presentation_kind,
        AddonMediaListPresentationKind::RemoteStreams
    );

    let form = repositories
        .addon_media_lists
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(form.addon_id, "org.example.lists");
    assert_eq!(form.source_url, "https://example.invalid/alice/watchlist");
    assert_eq!(
        form.requested_presentation_kind,
        AddonMediaListPresentationKind::RemoteStreams
    );
}

#[tokio::test]
async fn addon_media_list_repository_sorts_before_paging_by_name_or_displayed_id() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonInstallation" (
              "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
              "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
           ) VALUES
              ('a.addon', '1.0.0', 1, 'package-a', 'content-a', '{}',
               'addons/a.addon', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
              ('b.addon', '1.0.0', 1, 'package-b', 'content-b', '{}',
               'addons/b.addon', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
              ('z.addon', '1.0.0', 1, 'package-z', 'content-z', '{}',
               'addons/z.addon', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name"
           ) VALUES
              (1, 'z.addon', 'a-provider', 'https://example.invalid/1', 'Beta'),
              (2, 'a.addon', 'z-provider', 'https://example.invalid/2', 'alpha'),
              (3, 'a.addon', 'a-provider', 'https://example.invalid/3', 'Alpha'),
              (4, 'b.addon', 'a-provider', 'https://example.invalid/4', 'aardvark')"#,
    )
    .await
    .unwrap();

    let list_ids = |rows: Vec<ersatzrs_core::repositories::AddonMediaListAdminRow>| {
        rows.into_iter().map(|row| row.id).collect::<Vec<_>>()
    };
    let (name_ascending, _) = repositories
        .addon_media_lists
        .list_admin(
            0,
            2,
            AddonMediaListAdminSortField::Name,
            AddonMediaListAdminSortDirection::Ascending,
        )
        .await
        .unwrap();
    assert_eq!(list_ids(name_ascending), [4, 2]);

    let (name_ascending_second_page, _) = repositories
        .addon_media_lists
        .list_admin(
            1,
            2,
            AddonMediaListAdminSortField::Name,
            AddonMediaListAdminSortDirection::Ascending,
        )
        .await
        .unwrap();
    assert_eq!(list_ids(name_ascending_second_page), [3, 1]);

    let (name_descending, _) = repositories
        .addon_media_lists
        .list_admin(
            0,
            4,
            AddonMediaListAdminSortField::Name,
            AddonMediaListAdminSortDirection::Descending,
        )
        .await
        .unwrap();
    assert_eq!(list_ids(name_descending), [1, 3, 2, 4]);

    let (id_ascending, _) = repositories
        .addon_media_lists
        .list_admin(
            0,
            4,
            AddonMediaListAdminSortField::Id,
            AddonMediaListAdminSortDirection::Ascending,
        )
        .await
        .unwrap();
    assert_eq!(list_ids(id_ascending), [3, 2, 4, 1]);

    let (id_descending, _) = repositories
        .addon_media_lists
        .list_admin(
            0,
            4,
            AddonMediaListAdminSortField::Id,
            AddonMediaListAdminSortDirection::Descending,
        )
        .await
        .unwrap();
    assert_eq!(list_ids(id_descending), [1, 4, 2, 3]);
}

#[tokio::test]
async fn publication_readiness_distinguishes_blockers_from_optional_review_work() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount",
              "ProviderMetadataJson"
           ) VALUES (
              1, 'org.example.lists', 'fixture/show',
              'https://example.invalid/show', 'Fixture Show', 0,
              '{"title":"Fixture Show"}'
           )"#,
    )
    .await
    .unwrap();

    let empty = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert!(empty
        .blockers
        .iter()
        .any(|blocker| blocker.kind == AddonMediaListPublicationBlockerKind::NoIncludedItems));

    seed_numbering_item(&pool, 10, 1, None, None, true, None, "First").await;
    pool.execute(
        r#"UPDATE "AddonMediaListItem"
           SET "Availability" = 2, "ProviderLiveness" = 2,
               "EffectiveLiveness" = 2
           WHERE "Id" = 10"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "AddonMediaListImportJob" (
              "AddonMediaListId", "Generation", "State", "RecordCapability",
              "DiscoveredCount", "TotalHint", "OverviewComplete",
              "CreatedAt", "UpdatedAt", "LastProgressAt", "CompletedAt"
           ) VALUES (
              1, 1, 3, 'media-list.list.v4', 1, 1, 1,
              '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z',
              '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z'
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "AddonMediaListDetailJob" (
              "AddonMediaListItemId", "AddonMediaListId", "ProviderHost", "State",
              "OverviewFingerprint", "OverviewRecordJson", "UpdatedAt"
           ) VALUES (
              10, 1, 'example.invalid', 1,
              'fingerprint', '{}', '2026-08-26T10:00:00Z'
           )"#,
    )
    .await
    .unwrap();

    let advisory = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert!(advisory.blockers.is_empty(), "{:#?}", advisory.blockers);
    assert_eq!(advisory.recommendation_count, 1);
    assert_eq!(advisory.optional_detail_total, 1);
    assert_eq!(advisory.optional_detail_pending, 1);
    assert!(advisory.can_start_publication());

    assert_unresolved_replacement_readiness(&pool, &repositories.addon_media_lists).await;
}

async fn assert_unresolved_replacement_readiness(
    pool: &sqlx::SqlitePool,
    repository: &impl AddonMediaListRepository,
) {
    // Portable replacement intent blocks even when the original source is usable
    // or would normally be omitted for an access restriction.
    for restriction in [None, Some(0_i32), Some(1_i32)] {
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "SavedReplacementJson"='{"file":null}',
            "Availability"=?, "AvailabilityReasonCode"=? WHERE "Id"=10"#,
        )
        .bind(i32::from(restriction.is_some()))
        .bind(restriction)
        .execute(pool)
        .await
        .unwrap();
        let item = repository.list_items(1).await.unwrap().remove(0);
        assert!(item.replacement_unresolved);
        assert!(item.replacement_media_item_id.is_none());
        assert!(item.replacement_media_version_id.is_none());
        let readiness = repository.publication_readiness(1).await.unwrap();
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.kind
                == AddonMediaListPublicationBlockerKind::ReplacementUnresolved));
        assert!(!readiness.can_start_publication());
    }
    repository.set_replacement(10, None).await.unwrap();
    assert!(!repository.list_items(1).await.unwrap()[0].replacement_unresolved);
}

#[tokio::test]
async fn television_readiness_recommends_absent_numbering_but_blocks_partial_numbering() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount",
              "ProviderMetadataJson", "RequestedPresentationKind"
           ) VALUES (
              1, 'org.example.lists', 'fixture/unnumbered',
              'https://example.invalid/unnumbered', 'Unnumbered fixture', 1,
              '{"title":"Unnumbered fixture"}', 1
           )"#,
    )
    .await
    .unwrap();
    seed_numbering_item(&pool, 10, 0, None, None, true, None, "First").await;
    pool.execute(
        r#"UPDATE "AddonMediaListItem"
           SET "Availability" = 0, "ProviderLiveness" = 2,
               "EffectiveLiveness" = 2
           WHERE "Id" = 10"#,
    )
    .await
    .unwrap();

    let unnumbered = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert!(unnumbered.blockers.is_empty(), "{:#?}", unnumbered.blockers);
    assert_eq!(unnumbered.recommendation_count, 1);
    assert!(unnumbered.can_start_publication());

    let sentinel = AddonMediaListNfoMetadata {
        title: "First".to_owned(),
        season: Some(0),
        episode: Some(0),
        ..AddonMediaListNfoMetadata::default()
    };
    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "Season" = 0, "Episode" = 0, "ProviderMetadataJson" = ?
           WHERE "Id" = 10"#,
    )
    .bind(serde_json::to_string(&sentinel).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let sentinel_unnumbered = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert!(
        sentinel_unnumbered.blockers.is_empty(),
        "{:#?}",
        sentinel_unnumbered.blockers
    );
    assert_eq!(sentinel_unnumbered.recommendation_count, 1);
    assert!(sentinel_unnumbered.can_start_publication());

    let partial = AddonMediaListNfoMetadata {
        title: "First".to_owned(),
        season: Some(1),
        episode: None,
        ..AddonMediaListNfoMetadata::default()
    };
    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "Season" = 1, "Episode" = NULL, "ProviderMetadataJson" = ?
           WHERE "Id" = 10"#,
    )
    .bind(serde_json::to_string(&partial).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let inconsistent = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert!(inconsistent.blockers.iter().any(|blocker| {
        blocker.kind == AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing
    }));
    assert_eq!(inconsistent.recommendation_count, 0);
    assert!(!inconsistent.can_start_publication());
}

#[tokio::test]
async fn publication_readiness_excludes_only_typed_access_restrictions() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount",
              "ProviderMetadataJson"
           ) VALUES (
              1, 'org.example.lists', 'fixture/restricted',
              'https://example.invalid/restricted', 'Restricted fixture', 2,
              '{"title":"Restricted fixture"}'
           )"#,
    )
    .await
    .unwrap();
    seed_numbering_item(&pool, 10, 0, None, None, true, None, "Restricted").await;
    seed_numbering_item(&pool, 11, 1, None, None, true, None, "Playable").await;
    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "Availability" = 1, "AvailabilityReason" = 'not_playable',
               "AvailabilityReasonCode" = ?, "ProviderLiveness" = 1,
               "EffectiveLiveness" = 1, "DurationSeconds" = 60
           WHERE "Id" = 10"#,
    )
    .bind(1_i32)
    .execute(&pool)
    .await
    .unwrap();
    pool.execute(
        r#"UPDATE "AddonMediaListItem"
           SET "Availability" = 0, "ProviderLiveness" = 1,
               "EffectiveLiveness" = 1, "DurationSeconds" = 60
           WHERE "Id" = 11"#,
    )
    .await
    .unwrap();

    let readiness = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert!(readiness.blockers.is_empty(), "{:#?}", readiness.blockers);
    assert_eq!(readiness.included_count, 1);
    assert_eq!(readiness.restricted_excluded_count, 1);
    assert_eq!(readiness.recommendation_count, 1);

    pool.execute(
        r#"UPDATE "AddonMediaListItem"
           SET "AvailabilityReasonCode" = 2 WHERE "Id" = 10"#,
    )
    .await
    .unwrap();
    let generic_unavailable = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert!(generic_unavailable.blockers.iter().any(|blocker| {
        blocker.kind == AddonMediaListPublicationBlockerKind::UnavailableWithoutReplacement
    }));
}

#[tokio::test]
async fn addon_media_list_repository_propagates_closed_pool_failures() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    pool.close().await;

    assert!(matches!(
        repositories
            .addon_media_lists
            .list_admin(
                0,
                10,
                AddonMediaListAdminSortField::Name,
                AddonMediaListAdminSortDirection::Ascending,
            )
            .await,
        Err(RepositoryError::Unavailable { .. })
    ));
}

#[tokio::test]
async fn publication_refuses_an_incomplete_resumable_overview() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount"
           ) VALUES (
              1, 'org.example.lists', 'pending:fixture',
              'https://example.invalid/large-list', 'Large list', 0
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "AddonMediaListImportJob" (
              "AddonMediaListId", "Generation", "State", "RecordCapability",
              "CreatedAt", "UpdatedAt", "LastProgressAt"
           ) VALUES (
              1, 1, 0, 'media-list.list.v5',
              '2026-08-23T10:00:00Z', '2026-08-23T10:00:00Z', '2026-08-23T10:00:00Z'
           )"#,
    )
    .await
    .unwrap();

    let readiness = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert!(readiness
        .blockers
        .iter()
        .any(|blocker| blocker.kind == AddonMediaListPublicationBlockerKind::OverviewIncomplete));

    let error = repositories
        .addon_media_lists
        .stage_publication(1)
        .await
        .expect_err("an incomplete overview must never publish");
    assert!(matches!(error, RepositoryError::Conflict(_)));
    assert!(error
        .to_string()
        .contains("complete add-on overview must be verified"));
}

#[tokio::test]
async fn publication_prepares_missing_finite_durations_through_durable_detail_jobs() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount",
              "ProviderMetadataJson"
           ) VALUES (
              1, 'org.example.lists', 'fixture/show',
              'https://example.invalid/show', 'Fixture Show', 1,
              '{"title":"Fixture Show"}'
           )"#,
    )
    .await
    .unwrap();
    seed_numbering_item(&pool, 10, 1, None, None, true, None, "First").await;
    seed_numbering_item(&pool, 11, 2, None, None, true, None, "Retired").await;
    pool.execute(
        r#"UPDATE "AddonMediaListItem"
           SET "InDraft" = 0, "Published" = 1,
               "ProviderLiveness" = 1, "EffectiveLiveness" = 1,
               "DurationSeconds" = NULL WHERE "Id" = 11"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"UPDATE "AddonMediaListItem"
           SET "ProviderLiveness" = 1, "EffectiveLiveness" = 1,
               "DurationSeconds" = NULL
           WHERE "Id" = 10"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "AddonMediaListDetailJob" (
              "AddonMediaListItemId", "AddonMediaListId", "ProviderHost",
              "OverviewFingerprint", "OverviewRecordJson", "UpdatedAt"
           ) VALUES (
              10, 1, 'example.invalid', 'fingerprint', '{}', '2026-08-26T10:00:00Z'
           ), (
              11, 1, 'example.invalid', 'retired-fingerprint', '{}', '2026-08-26T10:00:00Z'
           )"#,
    )
    .await
    .unwrap();

    let before = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert!(before.blockers.is_empty(), "{:#?}", before.blockers);
    assert_eq!(before.required_playback_detail_count, 1);
    assert_eq!(before.required_playback_detail_pending, 0);
    assert!(before.needs_playback_preparation());

    assert_eq!(
        repositories
            .addon_media_lists
            .prepare_publication_details(1)
            .await
            .unwrap(),
        1
    );
    let queued: (i32, bool, Option<String>) = sqlx::query_as(
        r#"SELECT "State", "ManualPriority", "RequestedAt"
           FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId" = 10"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(queued.0, 1);
    assert!(queued.1);
    assert!(queued.2.is_some());
    let retired: (i32, bool, Option<String>) = sqlx::query_as(
        r#"SELECT "State", "ManualPriority", "RequestedAt"
           FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId" = 11"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retired, (0, false, None));
    let preparing = repositories
        .addon_media_lists
        .publication_readiness(1)
        .await
        .unwrap();
    assert_eq!(preparing.required_playback_detail_pending, 1);

    pool.execute(r#"UPDATE "AddonMediaListItem" SET "DurationSeconds" = 3600 WHERE "Id" = 10"#)
        .await
        .unwrap();
    assert_eq!(
        repositories
            .addon_media_lists
            .prepare_publication_details(1)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn episode_numbering_uses_provider_order_and_preserves_other_operator_metadata() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
              "ItemCount", "ReviewState"
           ) VALUES (
              1, 'org.example.lists', 'fixture/show',
              'https://example.invalid/show', 'Fixture Show', 5, 2
           )"#,
    )
    .await
    .unwrap();

    seed_numbering_item(&pool, 14, 1, None, None, true, None, "First").await;
    seed_numbering_item(&pool, 10, 2, None, None, true, None, "Second").await;
    seed_numbering_item(&pool, 11, 3, Some(2), Some(3), true, None, "Numbered").await;
    seed_numbering_item(&pool, 12, 4, None, None, false, None, "Excluded").await;
    seed_numbering_item(&pool, 13, 5, None, None, true, Some("parent"), "Chapter").await;
    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "MetadataOverrideJson" = '{"title":"Operator title"}',
               "MetadataConflictJson" = '{
                 "season":{"provider_before":1,"provider_after":null,"operator_value":2},
                 "plot":{"provider_before":"old","provider_after":"new","operator_value":"mine"}
               }'
           WHERE "Id" = 10"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let assigned = repositories
        .addon_media_lists
        .assign_episode_numbers(1, 2, 3, Some(2), true)
        .await
        .unwrap();
    assert_eq!(assigned, 2);

    let rows = repositories.addon_media_lists.list_items(1).await.unwrap();
    let row = |id| rows.iter().find(|row| row.id == id).unwrap();
    assert_eq!((row(14).season, row(14).episode), (Some(2), Some(4)));
    assert_eq!((row(10).season, row(10).episode), (Some(2), Some(5)));
    assert_eq!((row(11).season, row(11).episode), (Some(2), Some(3)));
    assert_eq!((row(12).season, row(12).episode), (None, None));
    assert_eq!((row(13).season, row(13).episode), (None, None));
    assert_eq!(row(10).title, "Operator title");

    let (overrides, conflicts): (String, Option<String>) = sqlx::query_as(
        r#"SELECT "MetadataOverrideJson", "MetadataConflictJson"
           FROM "AddonMediaListItem" WHERE "Id" = 10"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let overrides: serde_json::Value = serde_json::from_str(&overrides).unwrap();
    assert_eq!(overrides["title"], "Operator title");
    assert_eq!(overrides["season"], 2);
    assert_eq!(overrides["episode"], 5);
    let conflicts: serde_json::Value = serde_json::from_str(&conflicts.unwrap()).unwrap();
    assert!(conflicts.get("season").is_none());
    assert!(conflicts.get("episode").is_none());
    assert!(conflicts.get("plot").is_some());

    let reassigned = repositories
        .addon_media_lists
        .assign_episode_numbers(1, 2, 5, Some(1), false)
        .await
        .unwrap();
    assert_eq!(reassigned, 1);
    let rows = repositories.addon_media_lists.list_items(1).await.unwrap();
    let row = |id| rows.iter().find(|row| row.id == id).unwrap();
    assert_eq!((row(14).season, row(14).episode), (Some(2), Some(6)));
    assert_eq!((row(10).season, row(10).episode), (Some(2), Some(5)));
    assert_eq!((row(11).season, row(11).episode), (Some(2), Some(3)));
    assert_eq!(
        sqlx::query_scalar::<_, i32>(
            r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = 1"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn quick_review_edits_and_undo_snapshots_are_atomic() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
              "ItemCount", "ReviewState"
           ) VALUES (
              1, 'org.example.lists', 'fixture/show',
              'https://example.invalid/show', 'Fixture Show', 2, 2
           )"#,
    )
    .await
    .unwrap();
    seed_numbering_item(&pool, 10, 1, None, None, true, None, "First").await;
    seed_numbering_item(&pool, 11, 2, None, None, true, None, "Second").await;
    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "ProviderLiveness" = 2, "EffectiveLiveness" = 2
           WHERE "AddonMediaListId" = 1"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let original = AddonMediaListItemReviewEdit {
        item_id: 10,
        content_kind: AddonMediaListContentKind::Auto,
        included: true,
        liveness_override: None,
        season: None,
        episode: None,
    };
    let changed = AddonMediaListItemReviewEdit {
        content_kind: AddonMediaListContentKind::TelevisionEpisode,
        included: false,
        liveness_override: Some(RemoteStreamLiveness::Finite),
        season: Some(3),
        episode: Some(8),
        ..original
    };
    assert_eq!(
        repositories
            .addon_media_lists
            .apply_item_review_edits(1, &[changed])
            .await
            .unwrap(),
        1
    );
    let rows = repositories.addon_media_lists.list_items(1).await.unwrap();
    let row = rows.iter().find(|row| row.id == 10).unwrap();
    assert_eq!(
        row.content_kind,
        AddonMediaListContentKind::TelevisionEpisode
    );
    assert!(!row.included);
    assert_eq!(row.liveness_override, Some(RemoteStreamLiveness::Finite));
    assert_eq!((row.season, row.episode), (Some(3), Some(8)));

    repositories
        .addon_media_lists
        .apply_item_review_edits(1, &[original])
        .await
        .unwrap();
    let rows = repositories.addon_media_lists.list_items(1).await.unwrap();
    let row = rows.iter().find(|row| row.id == 10).unwrap();
    assert_eq!(row.content_kind, AddonMediaListContentKind::Auto);
    assert!(row.included);
    assert_eq!(row.liveness_override, None);
    assert_eq!((row.season, row.episode), (None, None));

    let error = repositories
        .addon_media_lists
        .apply_item_review_edits(1, &[changed, changed])
        .await
        .expect_err("duplicate identities must reject the whole transaction");
    assert!(matches!(error, RepositoryError::Conflict(_)));
}

async fn seed_addon(pool: &sqlx::SqlitePool) {
    pool.execute(
        r#"INSERT INTO "AddonInstallation" (
            "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
            "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
        ) VALUES (
            'org.example.lists', '1.0.0', 1, 'package', 'content', '{}',
            'addons/org.example.lists', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'
        )"#,
    )
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn seed_numbering_item(
    pool: &sqlx::SqlitePool,
    id: i64,
    rank: i32,
    season: Option<i32>,
    episode: Option<i32>,
    included: bool,
    parent_provider_id: Option<&str>,
    title: &str,
) {
    let metadata = AddonMediaListNfoMetadata {
        title: title.to_owned(),
        season,
        episode,
        ..AddonMediaListNfoMetadata::default()
    };
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItem" (
              "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
              "Rank", "Title", "Season", "Episode", "Included", "ParentProviderId",
              "ProviderMetadataJson"
           ) VALUES (?, 1, ?, ?, 4, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(id)
    .bind(format!("episode:{id}"))
    .bind(title)
    .bind(rank)
    .bind(title)
    .bind(season)
    .bind(episode)
    .bind(included)
    .bind(parent_provider_id)
    .bind(serde_json::to_string(&metadata).unwrap())
    .execute(pool)
    .await
    .unwrap();
}

/// The Local replacement picker matches a candidate on its persisted video
/// stream. ErsatzTV's `MediaStreamKind` is 1-based (`Video = 1`), so a fixture
/// or scanner that writes anything else makes the search return nothing at all
/// — which is how this feature once shipped completely unusable, and how a
/// browser spec seeded an invalid `0` and still looked like a product bug.
#[tokio::test]
async fn replacement_candidates_match_the_persisted_video_stream_kind() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_local_movie(
        &pool,
        1,
        "Replacement Candidate",
        ersatztv_video_stream_kind(),
    )
    .await;

    let candidates = repositories
        .addon_media_lists
        .search_replacement_candidates("Replacement Candidate", 10)
        .await
        .unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "the seeded local movie must be offered"
    );
    assert_eq!(candidates[0].title, "Replacement Candidate");
}

/// The same search must ignore an item whose only stream is audio, so a
/// music-only item is never offered as a video replacement.
#[tokio::test]
async fn replacement_candidates_ignore_items_without_a_video_stream() {
    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_local_movie(&pool, 1, "Audio Only", ersatztv_audio_stream_kind()).await;

    let candidates = repositories
        .addon_media_lists
        .search_replacement_candidates("Audio Only", 10)
        .await
        .unwrap();
    assert!(
        candidates.is_empty(),
        "an item with no video stream must not be offered: {candidates:?}"
    );
}

/// `ErsatzTV.Core/Domain/MediaItem/MediaStreamKind.cs` — the persisted enum is
/// 1-based, so `Video` is 1 and never 0.
const fn ersatztv_video_stream_kind() -> i32 {
    1
}

const fn ersatztv_audio_stream_kind() -> i32 {
    2
}

async fn seed_local_movie(pool: &sqlx::SqlitePool, id: i64, title: &str, stream_kind: i32) {
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
           VALUES (1, 0, 1, 'Movies')"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/movies', 1)"#,
    )
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "MovieMetadata" ("MovieId", "MetadataKind", "Title", "DateAdded", "DateUpdated")
           VALUES (?, 1, ?, '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z')"#,
    )
    .bind(id)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
              "Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width",
              "VideoScanKind", "MovieId"
           ) VALUES (?, '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z', '01:30:00', 1080, 1920, 0, ?)"#,
    )
    .bind(id)
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash") VALUES (?, ?, ?)"#,
    )
    .bind(id)
    .bind(format!("/movies/{title}.mkv"))
    .bind(format!("hash-{id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaStream" (
              "Index", "Codec", "MediaStreamKind", "Channels", "Default", "Forced",
              "MediaVersionId", "AttachedPic"
           ) VALUES (0, 'h264', ?, 0, 1, 0, ?, 0)"#,
    )
    .bind(stream_kind)
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

/// The editor renders one section per availability group, so each group must be
/// countable and pageable on its own. A regression here shows up as a section
/// listing another group's entries, or as a count that disagrees with its rows.
#[tokio::test]
async fn item_sections_count_and_page_each_availability_group_separately() {
    use ersatzrs_domain::AddonMediaListItemAvailability;

    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
            "AutoRefresh", "GeneratePlaylist", "ItemCount"
           ) VALUES (
            1, 'org.example.lists', 'alice/mixed',
            'https://example.invalid/alice/mixed', 'Mixed', 0, 0, 6
           )"#,
    )
    .await
    .unwrap();
    // Three unavailable, two available, one unknown, deliberately interleaved so
    // a filter that ignored availability would still return the first rows.
    for (id, rank, availability) in [
        (10, 1, 1),
        (11, 2, 0),
        (12, 3, 1),
        (13, 4, 2),
        (14, 5, 0),
        (15, 6, 1),
    ] {
        pool.execute(
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItem" (
                    "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
                    "Rank", "Title", "MediaItemId", "Availability"
                   ) VALUES (?, 1, ?, ?, 0, ?, 'Item', NULL, ?)"#,
            )
            .bind(id)
            .bind(format!("item:{id}"))
            .bind(format!("Item {id}"))
            .bind(rank)
            .bind(availability),
        )
        .await
        .unwrap();
    }

    let counts = repositories.addon_media_lists.item_counts(1).await.unwrap();
    assert_eq!(counts.unavailable, 3);
    assert_eq!(counts.available, 2);
    assert_eq!(counts.unknown, 1);

    let first = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Unavailable, 0, Some(2))
        .await
        .unwrap();
    assert_eq!(
        first.iter().map(|item| item.id).collect::<Vec<_>>(),
        [10, 12]
    );
    let second = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Unavailable, 1, Some(2))
        .await
        .unwrap();
    assert_eq!(second.iter().map(|item| item.id).collect::<Vec<_>>(), [15]);

    // A missing page size means "every row in this group", which the editor
    // offers explicitly and must not silently truncate.
    let all = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Unavailable, 0, None)
        .await
        .unwrap();
    assert_eq!(all.len(), 3);

    let unknown = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Unknown, 0, Some(10))
        .await
        .unwrap();
    assert_eq!(unknown.iter().map(|item| item.id).collect::<Vec<_>>(), [13]);
}

/// The entry owns its classification; the list value is only a bulk default.
/// Before this, `list_items` never read the column at all, so an add-on that
/// classified every entry correctly still showed up as unclassified.
#[tokio::test]
async fn item_content_kind_is_readable_settable_and_bulk_appliable() {
    use ersatzrs_domain::{AddonMediaListContentKind, AddonMediaListItemAvailability};

    let pool = common::test_db().await;
    let repositories = make_repositories(pool.clone());
    seed_addon(&pool).await;
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "ProviderId", "SourceUrl", "Name",
            "AutoRefresh", "GeneratePlaylist", "ItemCount", "ContentKindOverride"
           ) VALUES (
            1, 'org.example.lists', 'alice/kinds',
            'https://example.invalid/alice/kinds', 'Kinds', 0, 0, 2, 0
           )"#,
    )
    .await
    .unwrap();
    // The add-on classified one entry and left the other unclassified.
    pool.execute(
        r#"INSERT INTO "AddonMediaListItem" (
            "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
            "Rank", "Title", "MediaItemId", "Availability", "ContentKind"
           ) VALUES
            (10, 1, 'item:10', 'Episode', 0, 1, 'Episode', NULL, 0, 1),
            (11, 1, 'item:11', 'Unknown kind', 0, 2, 'Unknown', NULL, 0, 0)"#,
    )
    .await
    .unwrap();

    let rows = repositories.addon_media_lists.list_items(1).await.unwrap();
    assert_eq!(
        rows.iter()
            .map(|item| item.content_kind)
            .collect::<Vec<_>>(),
        [
            AddonMediaListContentKind::TelevisionEpisode,
            AddonMediaListContentKind::Auto
        ]
    );

    repositories
        .addon_media_lists
        .update_item_content_kind(11, AddonMediaListContentKind::Song)
        .await
        .unwrap();
    let rows = repositories
        .addon_media_lists
        .list_items_page(1, AddonMediaListItemAvailability::Available, 0, None)
        .await
        .unwrap();
    assert_eq!(rows[1].content_kind, AddonMediaListContentKind::Song);

    // The bulk default overwrites every entry and is stored on the list too, so
    // entries enumerated later inherit it.
    let updated = repositories
        .addon_media_lists
        .apply_list_content_kind(1, AddonMediaListContentKind::Movie)
        .await
        .unwrap();
    assert_eq!(updated, 2);
    let rows = repositories.addon_media_lists.list_items(1).await.unwrap();
    assert!(rows
        .iter()
        .all(|item| item.content_kind == AddonMediaListContentKind::Movie));
    assert_eq!(
        repositories
            .addon_media_lists
            .get_admin_form(1)
            .await
            .unwrap()
            .unwrap()
            .content_kind_override,
        AddonMediaListContentKind::Movie
    );

    // A missing entry is reported rather than silently ignored.
    assert!(matches!(
        repositories
            .addon_media_lists
            .update_item_content_kind(999, AddonMediaListContentKind::Movie)
            .await,
        Err(RepositoryError::NotFound { .. })
    ));
}
