use std::time::Duration;

use ersatzrs_core::repositories::{ChannelRepository as _, FfmpegProfileRepository as _};
use ersatzrs_domain::{Channel, FFmpegProfile};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

use super::*;

// Synthetic opaque envelope: mixed original v1/v2 documents, not active pages.
const DOCUMENT: &[u8] = b"{\"version\":1,\"sources\":[{\"schema\":1,\"bytes\":\"base\"},{\"schema\":2,\"bytes\":\"authored\"}]}\r\n";

async fn open(path: &std::path::Path) -> DbPool {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(10));
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap()
}

async fn fixture() -> (tempfile::TempDir, SqliteTeletextDesignRepository) {
    let directory = tempfile::tempdir().unwrap();
    let pool = open(&directory.path().join("designs.sqlite")).await;
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    (directory, SqliteTeletextDesignRepository(pool))
}

fn request(existing: Option<(i64, i64)>) -> TeletextDesignSave {
    TeletextDesignSave {
        name: "Synthetic mixed design".to_owned(),
        document_version: 1,
        document: DOCUMENT.to_vec(),
        existing,
    }
}

fn saved(result: M<TeletextDesignHead>) -> TeletextDesignHead {
    match result {
        M::Saved(head) => head,
        M::Rejected(reason) => panic!("unexpected {reason:?}"),
    }
}

async fn revisions(repo: &SqliteTeletextDesignRepository) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignRevision""#)
        .fetch_one(&repo.0)
        .await
        .unwrap()
}

#[tokio::test]
async fn exact_mixed_documents_survive_file_reopen_without_activation() {
    let (directory, repo) = fixture().await;
    let first = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let mut edit = request(Some((first.id, 1)));
    edit.name = "Synthetic revised design".to_owned();
    edit.document.extend_from_slice(b" ");
    let second = saved(repo.save(&edit, edit.document.len()).await.unwrap());
    assert_eq!(second.revision, 2);
    repo.0.close().await;
    let reopened =
        SqliteTeletextDesignRepository(open(&directory.path().join("designs.sqlite")).await);
    let original = reopened
        .revision(first.id, 1, DOCUMENT.len())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(original.document, DOCUMENT);
    assert_eq!(original.document_version, 1);
    assert_eq!(
        original.digest,
        "bf963dbbcc648cd9be1b37a58c76bbd497cfb53a94176644ba8993501ae05cb7"
    );
    assert_eq!(
        reopened
            .revision(first.id, 2, edit.document.len())
            .await
            .unwrap()
            .unwrap()
            .document,
        edit.document
    );
    assert_eq!(reopened.head(first.id).await.unwrap(), Some(second));
    for query in [
        r#"SELECT COUNT(*) FROM "ChannelTeletextBinding""#,
        r#"SELECT COUNT(*) FROM "TeletextProfile""#,
    ] {
        let count: i64 = sqlx::query_scalar(query)
            .fetch_one(&reopened.0)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn independent_connections_cas_exactly_one_winner_and_retain_old_revision() {
    let (directory, repo) = fixture().await;
    let first = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let other =
        SqliteTeletextDesignRepository(open(&directory.path().join("designs.sqlite")).await);
    let mut left = request(Some((first.id, 1)));
    left.document = b"synthetic left".to_vec();
    let mut right = request(Some((first.id, 1)));
    right.document = b"synthetic right".to_vec();
    let (a, b) = tokio::join!(repo.save(&left, 1024), other.save(&right, 1024));
    let results = [a.unwrap(), b.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, M::Saved(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, M::Rejected(R::Conflict)))
            .count(),
        1
    );
    assert_eq!(revisions(&repo).await, 2);
    assert_eq!(repo.head(first.id).await.unwrap().unwrap().revision, 2);
    assert_eq!(
        repo.revision(first.id, 1, DOCUMENT.len())
            .await
            .unwrap()
            .unwrap()
            .document,
        DOCUMENT
    );
    let latest = repo
        .revision(first.id, 2, 1024)
        .await
        .unwrap()
        .unwrap()
        .document;
    assert!(latest == left.document || latest == right.document);
}

#[tokio::test]
async fn invalid_bounds_stale_missing_and_overflow_do_not_advance_heads() {
    let (_directory, repo) = fixture().await;
    let mut invalid_requests = vec![request(None); 7];
    invalid_requests[0].name.clear();
    invalid_requests[1].name = "é".repeat(129);
    invalid_requests[2].name = "synthetic\0name".to_owned();
    invalid_requests[3].document_version = 2;
    invalid_requests[4].document.clear();
    invalid_requests[5].existing = Some((0, 1));
    invalid_requests[6].name = "\u{2003}".to_owned();
    for invalid_request in invalid_requests {
        assert_eq!(
            repo.save(&invalid_request, 1024).await.unwrap(),
            M::Rejected(R::InvalidValues)
        );
    }
    assert_eq!(
        repo.save(&request(None), DOCUMENT.len() - 1).await.unwrap(),
        M::Rejected(R::InvalidValues)
    );
    assert_eq!(
        repo.save(&request(None), 0).await.unwrap(),
        M::Rejected(R::InvalidValues)
    );
    assert_eq!(revisions(&repo).await, 0);
    let mut exact_name = request(None);
    exact_name.name = "é".repeat(128);
    let first = saved(repo.save(&exact_name, DOCUMENT.len()).await.unwrap());
    assert_eq!(first.name.len(), MAX_TELETEXT_DESIGN_NAME_BYTES);
    assert_eq!(
        repo.save(&request(Some((first.id, 2))), 1024)
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    assert_eq!(
        repo.save(&request(Some((first.id + 1, 1))), 1024)
            .await
            .unwrap(),
        M::Rejected(R::MissingDesign)
    );
    sqlx::query(r#"UPDATE "TeletextDesign" SET "Revision" = ? WHERE "Id" = ?"#)
        .bind(i64::MAX)
        .bind(first.id)
        .execute(&repo.0)
        .await
        .unwrap();
    assert_eq!(
        repo.save(&request(Some((first.id, i64::MAX))), 1024)
            .await
            .unwrap(),
        M::Rejected(R::RevisionLimit)
    );
    assert_eq!(
        repo.head(first.id).await.unwrap().unwrap().revision,
        i64::MAX
    );
    assert_eq!(revisions(&repo).await, 1);
}

#[tokio::test]
async fn tombstones_and_database_guards_preserve_immutable_bytes() {
    let (_directory, repo) = fixture().await;
    let first = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    assert_eq!(
        repo.tombstone(first.id, 2).await.unwrap(),
        M::Rejected(R::Conflict)
    );
    let deleted = saved(repo.tombstone(first.id, 1).await.unwrap());
    assert!(deleted.deleted);
    assert_eq!(deleted.revision, 1);
    assert_eq!(
        repo.save(&request(Some((first.id, 1))), 1024)
            .await
            .unwrap(),
        M::Rejected(R::DeletedDesign)
    );
    assert_eq!(
        repo.tombstone(first.id, 1).await.unwrap(),
        M::Rejected(R::DeletedDesign)
    );
    for statement in [
        r#"DELETE FROM "TeletextDesign""#,
        r#"DELETE FROM "TeletextDesignRevision""#,
        r#"UPDATE "TeletextDesignRevision" SET "Document" = X'00'"#,
    ] {
        assert!(sqlx::query(statement).execute(&repo.0).await.is_err());
    }
    assert_eq!(repo.head(first.id).await.unwrap(), Some(deleted));
    assert_eq!(
        repo.revision(first.id, 1, DOCUMENT.len())
            .await
            .unwrap()
            .unwrap()
            .document,
        DOCUMENT
    );
    assert_eq!(revisions(&repo).await, 1);
}

#[tokio::test]
async fn corrupt_or_oversized_stored_documents_are_errors_not_absence() {
    let (_directory, repo) = fixture().await;
    let first = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    assert!(matches!(
        repo.revision(first.id, 1, DOCUMENT.len() - 1).await,
        Err(RepositoryError::Decode { .. })
    ));
    assert!(repo.revision(first.id + 1, 1, 0).await.unwrap().is_none());
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
        .execute(&repo.0)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&repo.0)
        .await
        .unwrap();
    for statement in [
        r#"UPDATE "TeletextDesignRevision" SET "Digest" = 'bad'"#,
        r#"UPDATE "TeletextDesignRevision" SET "Digest" = printf('%064d', 0)"#,
        r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion" = 2"#,
        r#"UPDATE "TeletextDesignRevision" SET "Document" = zeroblob(2048)"#,
    ] {
        sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "Digest" = ?, "DocumentVersion" = 1, "Document" = ?"#)
            .bind(sha256_hex(DOCUMENT)).bind(DOCUMENT).execute(&repo.0).await.unwrap();
        sqlx::query(statement).execute(&repo.0).await.unwrap();
        assert!(matches!(
            repo.revision(first.id, 1, 1024).await,
            Err(RepositoryError::Decode { .. })
        ));
        assert_eq!(repo.head(first.id).await.unwrap(), Some(first.clone()));
        assert_eq!(revisions(&repo).await, 1);
    }
}

#[tokio::test]
async fn listing_is_keyset_bounded_and_corrupt_head_does_not_disappear() {
    let (_directory, repo) = fixture().await;
    let first = saved(repo.save(&request(None), 1024).await.unwrap());
    let second = saved(repo.save(&request(None), 1024).await.unwrap());
    let third = saved(repo.save(&request(None), 1024).await.unwrap());
    repo.tombstone(second.id, 1).await.unwrap();
    assert_eq!(repo.heads(None, 1).await.unwrap(), vec![first]);
    let tail = repo.heads(Some(second.id), 1).await.unwrap();
    assert_eq!(tail, vec![third.clone()]);
    assert!(repo.heads(None, 0).await.unwrap().is_empty());
    assert_eq!(repo.heads(None, 3).await.unwrap().len(), 3);
    assert!(repo.heads(None, usize::MAX).await.is_err());
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextDesign" SET "Name" = zeroblob(4096) WHERE "Id" = ?"#)
        .bind(third.id)
        .execute(&repo.0)
        .await
        .unwrap();
    assert!(matches!(
        repo.head(third.id).await,
        Err(RepositoryError::Decode { .. })
    ));
    assert!(matches!(
        repo.heads(Some(second.id), 1).await,
        Err(RepositoryError::Decode { .. })
    ));
}

#[tokio::test]
async fn producer_model_removal_does_not_delete_design_or_document() {
    let (_directory, repo) = fixture().await;
    let first = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    sqlx::query(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'synthetic', 1920, 1080)"#)
        .execute(&repo.0).await.unwrap();
    let repositories = super::super::make_repositories(repo.0.clone());
    repositories
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "Synthetic".to_owned(),
            resolution_id: 1,
            ..FFmpegProfile::default()
        })
        .await
        .unwrap();
    repositories
        .channel
        .upsert(&Channel {
            id: 1,
            unique_id: uuid::Uuid::new_v4(),
            number: "1".to_owned(),
            name: "Synthetic".to_owned(),
            ffmpeg_profile_id: 1,
            ..Channel::default()
        })
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision")
        VALUES ('org.example.design-fixture', '1.0.0', 0, 'synthetic', 'synthetic', '{}', 'fixture', 'synthetic', 'synthetic', 'synthetic')"#)
        .execute(&repo.0).await.unwrap();
    sqlx::query(r#"INSERT INTO "TeletextProfile" ("AddonId", "Name", "InstallationStateRevision", "InstallationVersion", "InstallationPackageSha256", "InstallationContentSha256", "InstallationManifestSha256")
        VALUES ('org.example.design-fixture', 'Synthetic', 'synthetic', '1.0.0', 'synthetic', 'synthetic', 'synthetic')"#)
        .execute(&repo.0).await.unwrap();
    for query in [
        r#"DELETE FROM "Channel""#,
        r#"DELETE FROM "TeletextProfile""#,
        r#"DELETE FROM "AddonInstallation""#,
    ] {
        sqlx::query(query).execute(&repo.0).await.unwrap();
    }
    assert_eq!(repo.head(first.id).await.unwrap(), Some(first.clone()));
    assert_eq!(
        repo.revision(first.id, 1, DOCUMENT.len())
            .await
            .unwrap()
            .unwrap()
            .document,
        DOCUMENT
    );
}

#[tokio::test]
async fn revision_insert_failure_rolls_back_both_new_and_existing_heads() {
    let (_directory, repo) = fixture().await;
    let first = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    sqlx::query(
        r#"CREATE TRIGGER "SyntheticRevisionFailure" BEFORE INSERT ON "TeletextDesignRevision"
        BEGIN SELECT RAISE(ABORT, 'synthetic revision failure'); END"#,
    )
    .execute(&repo.0)
    .await
    .unwrap();
    assert!(repo.save(&request(None), 1024).await.is_err());
    let mut edit = request(Some((first.id, 1)));
    edit.name = "Synthetic failed replacement".to_owned();
    assert!(repo.save(&edit, 1024).await.is_err());
    assert_eq!(repo.head(first.id).await.unwrap(), Some(first));
    assert_eq!(repo.heads(None, 10).await.unwrap().len(), 1);
    assert_eq!(revisions(&repo).await, 1);
}

#[tokio::test]
async fn corrupt_numeric_fields_are_null_projected_before_decode_and_never_mutated() {
    let (_directory, repo) = fixture().await;
    let first = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&repo.0)
        .await
        .unwrap();
    for (field, update) in [
        (
            "Revision",
            r#"UPDATE "TeletextDesign" SET "Revision" = ? WHERE "Id" = ?"#,
        ),
        (
            "IsDeleted",
            r#"UPDATE "TeletextDesign" SET "IsDeleted" = ? WHERE "Id" = ?"#,
        ),
    ] {
        for text in [false, true] {
            let query = sqlx::query(update);
            let query = if text {
                query.bind("corrupt-numeric-private-sentinel".repeat(4096))
            } else {
                query.bind(vec![0_u8; 131_072])
            };
            query.bind(first.id).execute(&repo.0).await.unwrap();
            let row = sqlx::query(head_sql!(r#"WHERE "Id" = ?"#))
                .bind(first.id)
                .fetch_one(&repo.0)
                .await
                .unwrap();
            assert_eq!(row.try_get::<Option<i64>, _>(field).unwrap(), None);
            assert!(matches!(
                repo.head(first.id).await,
                Err(RepositoryError::Decode { .. })
            ));
            assert!(matches!(
                repo.heads(None, 1).await,
                Err(RepositoryError::Decode { .. })
            ));
            assert!(matches!(
                repo.save(&request(Some((first.id, 1))), 1024).await,
                Err(RepositoryError::Decode { .. })
            ));
            assert!(matches!(
                repo.tombstone(first.id, 1).await,
                Err(RepositoryError::Decode { .. })
            ));
            assert_eq!(revisions(&repo).await, 1);
            sqlx::query(
                r#"UPDATE "TeletextDesign" SET "Revision" = 1, "IsDeleted" = 0 WHERE "Id" = ?"#,
            )
            .bind(first.id)
            .execute(&repo.0)
            .await
            .unwrap();
        }
    }
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
        .execute(&repo.0)
        .await
        .unwrap();
    for text in [false, true] {
        let query = sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion" = ?"#);
        let query = if text {
            query.bind("corrupt-version-private-sentinel".repeat(4096))
        } else {
            query.bind(vec![0_u8; 131_072])
        };
        query.execute(&repo.0).await.unwrap();
        let error = repo
            .revision(first.id, 1, DOCUMENT.len())
            .await
            .unwrap_err();
        assert!(matches!(error, RepositoryError::Decode { .. }));
        assert!(!error.to_string().contains("private-sentinel"));
        assert_eq!(repo.head(first.id).await.unwrap(), Some(first.clone()));
        assert_eq!(revisions(&repo).await, 1);
    }
}
