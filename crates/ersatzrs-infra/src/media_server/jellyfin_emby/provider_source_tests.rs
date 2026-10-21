//! Selected provider-source identity stays atomic with the owning media file.

use sqlx::{migrate::Migrator, sqlite::SqlitePoolOptions, Executor as _};

use super::*;

const MIGRATION: i64 = 202_610_211_634;
const PATH: &str = "fixture/selected.ts";
type StoredFile = (i64, i64, String, String, Option<String>);

async fn fixture(before_migration: bool) -> DbPool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys=ON").await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| !before_migration || migration.version < MIGRATION)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO "MediaSource" ("Id") VALUES (11);
        INSERT INTO "JellyfinMediaSource" ("Id") VALUES (11);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (21,'Synthetic',1,11);
        INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (21,1);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (31,21,'fixture');
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (101,31);
        INSERT INTO "Movie" ("Id") VALUES (101);
        INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind")
            VALUES (301,101,'2026-01-01','2026-01-01','00:00:12',90,160,0);
        INSERT INTO "MediaStream" ("Id","MediaVersionId","Index","Codec","MediaStreamKind","Channels","Default","Forced")
            VALUES (401,301,3,'dvb_teletext',3,0,0,0);"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    for (id, path) in [(501, PATH), (502, "fixture/foreign.ts")] {
        sqlx::query(r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (?,301,?,?)"#)
            .bind(id).bind(path).bind(path_hash(path)).execute(&pool).await.unwrap();
        seed_report(&pool, id).await;
    }
    pool
}

async fn seed_report(pool: &DbPool, file: i64) {
    sqlx::query(
        r#"INSERT INTO "SourceTeletextMetadata" (
            "Id","MediaVersionId","MediaStreamId","MediaFileId","StreamIndex",
            "BindingStatus","Provenance","PidStatus","ProgramStatus","DescriptorStatus","ObservedPagesStatus")
            VALUES (?,301,401,?,3,0,0,0,0,2,0)"#,
    ).bind(file).bind(file).execute(pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "SourceTeletextDescriptor" ("SourceTeletextMetadataId","ProjectionOrdinal","Language","DescriptorType","WireMagazine","WirePage")
        VALUES (?,0,'eng',2,0,136)"#)
        .bind(file).execute(pool).await.unwrap();
}

async fn reports(pool: &DbPool) -> Vec<(i64, i64, String)> {
    sqlx::query_as(r#"SELECT m."MediaFileId",d."ProjectionOrdinal",d."Language"
        FROM "SourceTeletextMetadata" m JOIN "SourceTeletextDescriptor" d ON d."SourceTeletextMetadataId"=m."Id"
        ORDER BY m."MediaFileId",d."ProjectionOrdinal";"#)
        .fetch_all(pool).await.unwrap()
}

fn streams() -> Vec<MediaStream> {
    vec![
        MediaStream {
            index: 0,
            kind: MediaStreamKind::Video,
            ..MediaStream::default()
        },
        MediaStream {
            index: 3,
            kind: MediaStreamKind::Subtitle,
            ..MediaStream::default()
        },
        // A sidecar may reuse an index without being part of the native layout.
        MediaStream {
            index: 0,
            kind: MediaStreamKind::ExternalSubtitle,
            ..MediaStream::default()
        },
    ]
}

fn declaration(id: &str) -> ProviderMediaSourceDeclaration {
    ProviderMediaSourceDeclaration::new(id.to_owned(), PATH.to_owned(), [Some(0), Some(3)]).unwrap()
}

async fn stored(pool: &DbPool) -> StoredFile {
    sqlx::query_as(r#"SELECT "Id","MediaVersionId","Path","PathHash","ProviderMediaSourceId" FROM "MediaFile" WHERE "Id"=501"#)
        .fetch_one(pool).await.unwrap()
}

async fn files(pool: &DbPool) -> Vec<StoredFile> {
    sqlx::query_as(
        r#"SELECT "Id","MediaVersionId","Path","PathHash","ProviderMediaSourceId" FROM "MediaFile" ORDER BY "Id""#,
    ).fetch_all(pool).await.unwrap()
}

async fn full_reports(pool: &DbPool) -> Vec<String> {
    sqlx::query_scalar(r#"SELECT json_array(m."Id",m."MediaVersionId",m."MediaStreamId",m."MediaFileId",
        m."StreamIndex",m."BindingStatus",m."MediaSourceId",m."Provider",m."SourceRevision",m."Provenance",
        m."PidStatus",m."Pid",m."ProgramStatus",m."DescriptorStatus",m."TrailingUnpairedPairs",m."ObservedPagesStatus",
        d."SourceTeletextMetadataId",d."ProjectionOrdinal",d."Language",d."DescriptorType",d."WireMagazine",d."WirePage")
        FROM "SourceTeletextMetadata" m LEFT JOIN "SourceTeletextDescriptor" d ON d."SourceTeletextMetadataId"=m."Id"
        ORDER BY m."Id",d."ProjectionOrdinal";"#)
        .fetch_all(pool).await.unwrap()
}

async fn another_version(pool: &DbPool) {
    sqlx::query(r#"INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind")
        VALUES (302,101,'2026-01-01','2026-01-01','00:00:12',90,160,0)"#)
        .execute(pool).await.unwrap();
}

#[tokio::test]
async fn provider_source_identity_migration_preserves_existing_files_and_reports() {
    let pool = fixture(true).await;
    let before: Vec<(i64, i64, String, String)> = sqlx::query_as(
        r#"SELECT "Id","MediaVersionId","Path","PathHash" FROM "MediaFile" ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let original_reports = reports(&pool).await;
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let after: Vec<(i64, i64, String, String)> = sqlx::query_as(
        r#"SELECT "Id","MediaVersionId","Path","PathHash" FROM "MediaFile" ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    let ids: Vec<Option<String>> =
        sqlx::query_scalar(r#"SELECT "ProviderMediaSourceId" FROM "MediaFile" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(ids, [None, None]);
    assert_eq!(reports(&pool).await, original_reports);
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn provider_source_identity_requires_exact_declared_path_and_embedded_indexes() {
    let pool = fixture(false).await;
    let original = stored(&pool).await;
    let valid = declaration("selected-source");
    let mut wrong_path = valid.clone();
    wrong_path.path = "fixture/other.ts".to_owned();
    let mut blank_id = valid.clone();
    blank_id.source_id = " \t".to_owned();
    let mut missing_index = valid.clone();
    missing_index.stream_indexes.pop();
    for invalid in [None, Some(wrong_path), Some(blank_id), Some(missing_index)] {
        for (declaration, expected) in [
            (Some(&valid), Some("selected-source")),
            (invalid.as_ref(), None),
        ] {
            let mut tx = crate::db::begin_write(&pool).await.unwrap();
            ensure_media_file(
                &mut tx,
                VersionOwner::Movie(101),
                301,
                PATH,
                declaration,
                &streams(),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            let actual = stored(&pool).await;
            assert_eq!(
                (&actual.0, &actual.1, &actual.2, &actual.3),
                (&original.0, &original.1, &original.2, &original.3)
            );
            assert_eq!(actual.4.as_deref(), expected);
        }
    }
}

#[tokio::test]
async fn provider_source_identity_changes_invalidate_only_own_reports_and_rollback_atomically() {
    let pool = fixture(false).await;
    let original = stored(&pool).await;
    let original_reports = reports(&pool).await;
    // NULL-to-NULL is a real no-op, including its report children.
    let mut tx = crate::db::begin_write(&pool).await.unwrap();
    ensure_media_file(
        &mut tx,
        VersionOwner::Movie(101),
        301,
        PATH,
        None,
        &streams(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(reports(&pool).await, original_reports);
    for wrong_owner in [VersionOwner::Movie(102), VersionOwner::Episode(101)] {
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        assert!(ensure_media_file(
            &mut tx,
            wrong_owner,
            301,
            PATH,
            Some(&declaration("wrong-owner")),
            &streams()
        )
        .await
        .is_err());
        tx.rollback().await.unwrap();
        assert_eq!(stored(&pool).await, original);
        assert_eq!(reports(&pool).await, original_reports);
    }
    let mut tx = crate::db::begin_write(&pool).await.unwrap();
    ensure_media_file(
        &mut tx,
        VersionOwner::Movie(101),
        301,
        PATH,
        Some(&declaration("source-a")),
        &streams(),
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(stored(&pool).await, original);
    assert_eq!(reports(&pool).await, original_reports);
    let mut tx = crate::db::begin_write(&pool).await.unwrap();
    ensure_media_file(
        &mut tx,
        VersionOwner::Movie(101),
        301,
        PATH,
        Some(&declaration("source-a")),
        &streams(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(reports(&pool).await, vec![(502, 0, "eng".to_owned())]);
    seed_report(&pool, 501).await;
    let mut tx = crate::db::begin_write(&pool).await.unwrap();
    ensure_media_file(
        &mut tx,
        VersionOwner::Movie(101),
        301,
        PATH,
        Some(&declaration("source-a")),
        &streams(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(reports(&pool).await, original_reports);
    for selected in [Some(declaration("source-b")), None] {
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        ensure_media_file(
            &mut tx,
            VersionOwner::Movie(101),
            301,
            PATH,
            selected.as_ref(),
            &streams(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(reports(&pool).await, vec![(502, 0, "eng".to_owned())]);
        seed_report(&pool, 501).await;
    }
}

#[tokio::test]
async fn provider_source_identity_isolates_versions_with_the_same_plain_path() {
    let pool = fixture(false).await;
    another_version(&pool).await;
    let before = files(&pool).await;
    let original_reports = full_reports(&pool).await;
    let selected = declaration("new-version-source");
    let mut tx = crate::db::begin_write(&pool).await.unwrap();
    ensure_media_file(
        &mut tx,
        VersionOwner::Movie(101),
        302,
        PATH,
        Some(&selected),
        &streams(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let first = files(&pool).await;
    assert_eq!(first.len(), before.len() + 1);
    for file in before {
        assert!(first.contains(&file));
    }
    let own = first.iter().find(|file| file.1 == 302).unwrap();
    assert_eq!(own.2, PATH);
    assert_ne!(own.3, path_hash(PATH));
    assert_eq!(own.4.as_deref(), Some("new-version-source"));
    assert_eq!(full_reports(&pool).await, original_reports);
    let mut tx = crate::db::begin_write(&pool).await.unwrap();
    ensure_media_file(
        &mut tx,
        VersionOwner::Movie(101),
        302,
        PATH,
        Some(&selected),
        &streams(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        files(&pool).await,
        first,
        "repeat scans preserve the new namespaced identity"
    );
    assert_eq!(full_reports(&pool).await, original_reports);
}

#[tokio::test]
async fn provider_source_identity_rejects_corrupt_foreign_namespaced_key() {
    let pool = fixture(false).await;
    another_version(&pool).await;
    let path = "fixture/collision.ts";
    sqlx::query(
        r#"UPDATE "MediaFile" SET "PathHash"=?,"ProviderMediaSourceId"='foreign-source' WHERE "Id"=502"#,
    )
    .bind(provider_media_file_hash(302, path))
    .execute(&pool)
    .await
    .unwrap();
    seed_report(&pool, 502).await;
    let before = files(&pool).await;
    let original_reports = full_reports(&pool).await;
    let mut selected = declaration("attempted-relabel");
    selected.path = path.to_owned();
    let mut tx = crate::db::begin_write(&pool).await.unwrap();
    let result = ensure_media_file(
        &mut tx,
        VersionOwner::Movie(101),
        302,
        &selected.path,
        Some(&selected),
        &streams(),
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Conflict(_))));
    tx.commit().await.unwrap();
    assert_eq!(
        files(&pool).await,
        before,
        "a refused write must not move or relabel another version's file"
    );
    assert_eq!(full_reports(&pool).await, original_reports);
}

#[tokio::test]
async fn provider_source_identity_rejects_ambiguous_owned_file_matches() {
    let pool = fixture(false).await;
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=502"#)
        .bind(PATH)
        .execute(&pool)
        .await
        .unwrap();
    seed_report(&pool, 502).await;
    let before = files(&pool).await;
    let original_reports = full_reports(&pool).await;
    let mut tx = crate::db::begin_write(&pool).await.unwrap();
    let result = ensure_media_file(
        &mut tx,
        VersionOwner::Movie(101),
        301,
        PATH,
        Some(&declaration("ambiguous")),
        &streams(),
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Conflict(_))));
    tx.commit().await.unwrap();
    assert_eq!(files(&pool).await, before);
    assert_eq!(full_reports(&pool).await, original_reports);
}

#[tokio::test]
async fn provider_source_identity_path_move_updates_selected_file_and_refuses_ambiguous_fallback() {
    let pool = fixture(false).await;
    sqlx::query(r#"DELETE FROM "MediaFile" WHERE "Id"=502"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "ProviderMediaSourceId"='old-source' WHERE "Id"=501"#)
        .execute(&pool)
        .await
        .unwrap();
    seed_report(&pool, 501).await;
    let original = stored(&pool).await;
    let moved = "fixture/moved.ts";
    let mut selected = declaration("moved-source");
    selected.path = moved.to_owned();
    for first_move in [true, false] {
        if !first_move {
            seed_report(&pool, 501).await;
        }
        let before_reports = full_reports(&pool).await;
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        ensure_media_file(
            &mut tx,
            VersionOwner::Movie(101),
            301,
            moved,
            Some(&selected),
            &streams(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let actual = stored(&pool).await;
        assert_eq!(
            (actual.0, actual.1, &actual.3),
            (original.0, original.1, &original.3)
        );
        assert_eq!(actual.2, moved);
        assert_eq!(actual.4.as_deref(), Some("moved-source"));
        assert_eq!(files(&pool).await.len(), 1);
        let bound = crate::next_engine::schedule::bind_media(
            &pool,
            ersatzrs_domain::PlayoutItem {
                media_item_id: 101,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (
                bound.media_file_id,
                bound.media_version_id,
                bound.path.as_str()
            ),
            (501, 301, moved)
        );
        let source: Option<String> = sqlx::query_scalar(r#"SELECT "ProviderMediaSourceId" FROM "MediaFile" WHERE "Id"=? AND "MediaVersionId"=? AND "Path"=?"#)
            .bind(bound.media_file_id).bind(bound.media_version_id).bind(&bound.path)
            .fetch_one(&pool).await.unwrap();
        assert_eq!(source.as_deref(), Some("moved-source"));
        assert_eq!(
            full_reports(&pool).await,
            if first_move { vec![] } else { before_reports }
        );
    }

    let pool = fixture(false).await;
    let before = files(&pool).await;
    let before_reports = full_reports(&pool).await;
    let mut tx = crate::db::begin_write(&pool).await.unwrap();
    let result = ensure_media_file(
        &mut tx,
        VersionOwner::Movie(101),
        301,
        moved,
        Some(&selected),
        &streams(),
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Conflict(_))));
    tx.commit().await.unwrap();
    assert_eq!(files(&pool).await, before);
    assert_eq!(full_reports(&pool).await, before_reports);
}
