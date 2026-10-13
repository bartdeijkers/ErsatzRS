use super::super::tests::{fixture, open, request, saved, DOCUMENT};
use super::*;

const RECEIPT: &[u8] = b"{\"version\":1,\"mapping\":\"synthetic original\"}\r\n";

fn export_request(head: &TeletextDesignHead) -> TeletextDesignExportSave {
    TeletextDesignExportSave {
        design_id: head.id,
        revision: 1,
        design_digest: sha256_hex(DOCUMENT),
        expected_design_head: head.revision,
        receipt_version: 1,
        receipt: RECEIPT.to_vec(),
    }
}

fn exported(result: M<TeletextDesignExportMetadata>) -> TeletextDesignExportMetadata {
    match result {
        M::Saved(metadata) => metadata,
        M::Rejected(reason) => panic!("unexpected {reason:?}"),
    }
}

async fn count(repo: &SqliteTeletextDesignRepository) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignExportReceipt""#)
        .fetch_one(&repo.0)
        .await
        .unwrap()
}

#[tokio::test]
async fn migration_and_exact_historical_exports_reopen_without_activation() {
    let (directory, repo) = fixture().await;
    let original = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let mut update = request(Some((original.id, 1)));
    update.document.extend_from_slice(b" second revision");
    let head = saved(repo.save(&update, 1024).await.unwrap());
    let metadata = exported(
        repo.create_export(&export_request(&head), RECEIPT.len(), DOCUMENT.len())
            .await
            .unwrap(),
    );
    assert_eq!(metadata.design_id, original.id);
    assert_eq!(metadata.revision, 1);
    assert_eq!(metadata.expected_design_head, 2);
    assert_eq!(metadata.design_digest, sha256_hex(DOCUMENT));
    assert_eq!(metadata.digest, sha256_hex(RECEIPT));
    assert_eq!(metadata.receipt_version, 1);
    assert_eq!(repo.head(original.id).await.unwrap(), Some(head.clone()));
    let triggers: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "sqlite_master" WHERE "type" = 'trigger' AND "tbl_name" = 'TeletextDesignExportReceipt'"#)
        .fetch_one(&repo.0).await.unwrap();
    assert_eq!(triggers, 2);
    let foreign_keys = sqlx::query(r#"PRAGMA foreign_key_list("TeletextDesignExportReceipt")"#)
        .fetch_all(&repo.0)
        .await
        .unwrap();
    assert_eq!(foreign_keys.len(), 2);
    for row in foreign_keys {
        assert_eq!(row.get::<String, _>("table"), "TeletextDesignRevision");
        assert_eq!(row.get::<String, _>("on_delete"), "RESTRICT");
    }
    repo.0.close().await;
    let reopened =
        SqliteTeletextDesignRepository(open(&directory.path().join("designs.sqlite")).await);
    let stored = reopened
        .export(metadata.id, RECEIPT.len(), DOCUMENT.len())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.metadata, metadata);
    assert_eq!(stored.receipt, RECEIPT);
    assert_eq!(reopened.head(original.id).await.unwrap(), Some(head));
    for table in [
        "TeletextPublicationRequest",
        "TeletextAcceptedGeneration",
        "ChannelTeletextBinding",
    ] {
        let query = match table {
            "TeletextPublicationRequest" => r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#,
            "TeletextAcceptedGeneration" => r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#,
            _ => r#"SELECT COUNT(*) FROM "ChannelTeletextBinding""#,
        };
        let rows: i64 = sqlx::query_scalar(query)
            .fetch_one(&reopened.0)
            .await
            .unwrap();
        assert_eq!(rows, 0);
    }
}

#[tokio::test]
async fn invalid_export_metadata_and_zero_or_overflow_bounds_never_write() {
    let (_directory, repo) = fixture().await;
    let head = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let valid = export_request(&head);
    let mut invalid = vec![valid.clone(); 8];
    invalid[0].design_id = 0;
    invalid[1].revision = 0;
    invalid[2].expected_design_head = 0;
    invalid[3].revision = 2;
    invalid[4].design_digest = "private corrupt digest".into();
    invalid[5].design_digest = "A".repeat(64);
    invalid[6].receipt_version = 2;
    invalid[7].receipt.clear();
    for save in invalid {
        assert_eq!(
            repo.create_export(&save, 1024, 1024).await.unwrap(),
            M::Rejected(R::InvalidValues)
        );
    }
    for (receipt_max, document_max) in [
        (0, 1024),
        (RECEIPT.len() - 1, 1024),
        (1024, 0),
        (usize::MAX, 1024),
        (1024, usize::MAX),
    ] {
        assert_eq!(
            repo.create_export(&valid, receipt_max, document_max)
                .await
                .unwrap(),
            M::Rejected(R::InvalidValues)
        );
    }
    assert_eq!(count(&repo).await, 0);
    assert_eq!(repo.head(head.id).await.unwrap(), Some(head));
}

#[tokio::test]
async fn stale_head_digest_missing_revision_and_deleted_exports_preserve_storage() {
    let (_directory, repo) = fixture().await;
    let head = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let mut save = export_request(&head);
    save.design_id += 1;
    assert_eq!(
        repo.create_export(&save, 1024, 1024).await.unwrap(),
        M::Rejected(R::MissingDesign)
    );
    save = export_request(&head);
    save.design_digest = "0".repeat(64);
    assert_eq!(
        repo.create_export(&save, 1024, 1024).await.unwrap(),
        M::Rejected(R::Conflict)
    );
    save = export_request(&head);
    save.expected_design_head = 2;
    assert_eq!(
        repo.create_export(&save, 1024, 1024).await.unwrap(),
        M::Rejected(R::Conflict)
    );
    let next = saved(
        repo.save(&request(Some((head.id, 1))), DOCUMENT.len())
            .await
            .unwrap(),
    );
    assert_eq!(
        repo.create_export(&export_request(&head), 1024, 1024)
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    // A synthetic malformed head permits exercising a genuinely missing target.
    sqlx::query(r#"UPDATE "TeletextDesign" SET "Revision" = 3 WHERE "Id" = ?"#)
        .bind(head.id)
        .execute(&repo.0)
        .await
        .unwrap();
    save = export_request(&next);
    save.expected_design_head = 3;
    save.revision = 3;
    assert_eq!(
        repo.create_export(&save, 1024, 1024).await.unwrap(),
        M::Rejected(R::MissingDesign)
    );
    repo.tombstone(head.id, 3).await.unwrap();
    assert_eq!(
        repo.create_export(&save, 1024, 1024).await.unwrap(),
        M::Rejected(R::DeletedDesign)
    );
    assert_eq!(count(&repo).await, 0);
    assert!(repo.head(head.id).await.unwrap().unwrap().deleted);
}

#[tokio::test]
async fn exports_do_not_advance_maximum_heads_and_identity_exhaustion_rejects() {
    let (_directory, repo) = fixture().await;
    let head = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    sqlx::query(r#"UPDATE "TeletextDesign" SET "Revision" = ? WHERE "Id" = ?"#)
        .bind(i64::MAX)
        .bind(head.id)
        .execute(&repo.0)
        .await
        .unwrap();
    let mut save = export_request(&head);
    save.expected_design_head = i64::MAX;
    let first = exported(repo.create_export(&save, 1024, 1024).await.unwrap());
    assert_eq!(first.expected_design_head, i64::MAX);
    sqlx::query(
        r#"UPDATE "sqlite_sequence" SET "seq" = ? WHERE "name" = 'TeletextDesignExportReceipt'"#,
    )
    .bind(i64::MAX)
    .execute(&repo.0)
    .await
    .unwrap();
    assert_eq!(
        repo.create_export(&save, 1024, 1024).await.unwrap(),
        M::Rejected(R::RevisionLimit)
    );
    assert_eq!(count(&repo).await, 1);
    assert_eq!(
        repo.head(head.id).await.unwrap().unwrap().revision,
        i64::MAX
    );
    assert_eq!(
        repo.export(first.id, 1024, 1024)
            .await
            .unwrap()
            .unwrap()
            .metadata,
        first
    );
}

#[tokio::test]
async fn export_insert_failure_rolls_back_and_preserves_head_and_existing_receipt() {
    let (_directory, repo) = fixture().await;
    let head = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let save = export_request(&head);
    let first = exported(repo.create_export(&save, 1024, 1024).await.unwrap());
    sqlx::query(
        r#"CREATE TRIGGER "SyntheticExportFailure" BEFORE INSERT ON "TeletextDesignExportReceipt"
        BEGIN SELECT RAISE(ABORT, 'synthetic export failure'); END"#,
    )
    .execute(&repo.0)
    .await
    .unwrap();
    assert!(repo.create_export(&save, 1024, 1024).await.is_err());
    assert_eq!(count(&repo).await, 1);
    assert_eq!(repo.head(head.id).await.unwrap(), Some(head));
    assert_eq!(
        repo.export(first.id, 1024, 1024)
            .await
            .unwrap()
            .unwrap()
            .metadata,
        first
    );
}

#[tokio::test]
async fn tombstones_and_immutable_guards_preserve_export_bytes() {
    let (_directory, repo) = fixture().await;
    let head = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let receipt = exported(
        repo.create_export(&export_request(&head), 1024, 1024)
            .await
            .unwrap(),
    );
    for statement in [
        r#"UPDATE "TeletextDesignExportReceipt" SET "Receipt" = X'00'"#,
        r#"DELETE FROM "TeletextDesignExportReceipt""#,
        r#"DELETE FROM "TeletextDesignRevision""#,
    ] {
        assert!(sqlx::query(statement).execute(&repo.0).await.is_err());
    }
    repo.tombstone(head.id, head.revision).await.unwrap();
    let stored = repo.export(receipt.id, 1024, 1024).await.unwrap().unwrap();
    assert_eq!(stored.metadata, receipt);
    assert_eq!(stored.receipt, RECEIPT);
    assert_eq!(count(&repo).await, 1);
}

#[tokio::test]
async fn export_reads_bound_receipt_and_original_and_distinguish_absence() {
    let (_directory, repo) = fixture().await;
    let head = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let receipt = exported(
        repo.create_export(&export_request(&head), 1024, 1024)
            .await
            .unwrap(),
    );
    for (receipt_max, document_max) in [
        (0, 1024),
        (RECEIPT.len() - 1, 1024),
        (1024, 0),
        (1024, DOCUMENT.len() - 1),
    ] {
        assert!(matches!(
            repo.export(receipt.id, receipt_max, document_max).await,
            Err(RepositoryError::Decode { .. })
        ));
    }
    assert!(repo.export(receipt.id + 1, 0, 0).await.unwrap().is_none());
    assert!(repo.export(0, 1024, 1024).await.is_err());
    assert!(repo.export(receipt.id, usize::MAX, 1024).await.is_err());
    assert!(repo.export(receipt.id, 1024, usize::MAX).await.is_err());
    assert!(matches!(
        repo.create_export(&export_request(&head), 1024, DOCUMENT.len() - 1)
            .await,
        Err(RepositoryError::Decode { .. })
    ));
    assert_eq!(count(&repo).await, 1);
}

#[tokio::test]
async fn corrupt_export_metadata_or_bytes_error_without_echoing_private_content() {
    let (_directory, repo) = fixture().await;
    let head = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let receipt = exported(
        repo.create_export(&export_request(&head), 1024, 1024)
            .await
            .unwrap(),
    );
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignExportReceipt_ImmutableUpdate""#)
        .execute(&repo.0)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&repo.0)
        .await
        .unwrap();
    // Deliberately damage this disposable single-connection database so reads
    // must fail closed even when SQLite's usual metadata guards were bypassed.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&repo.0)
        .await
        .unwrap();
    for statement in [
        r#"UPDATE "TeletextDesignExportReceipt" SET "Digest" = 'private corrupt digest'"#,
        r#"UPDATE "TeletextDesignExportReceipt" SET "Digest" = printf('%064d', 0)"#,
        r#"UPDATE "TeletextDesignExportReceipt" SET "DesignDigest" = printf('%064d', 0)"#,
        r#"UPDATE "TeletextDesignExportReceipt" SET "ReceiptVersion" = 2"#,
        r#"UPDATE "TeletextDesignExportReceipt" SET "ReceiptVersion" = zeroblob(131072)"#,
        r#"UPDATE "TeletextDesignExportReceipt" SET "DesignId" = zeroblob(131072)"#,
        r#"UPDATE "TeletextDesignExportReceipt" SET "Revision" = 'private corrupt revision'"#,
        r#"UPDATE "TeletextDesignExportReceipt" SET "ExpectedDesignHead" = zeroblob(131072)"#,
        r#"UPDATE "TeletextDesignExportReceipt" SET "Receipt" = zeroblob(2048)"#,
        r#"UPDATE "TeletextDesignExportReceipt" SET "Receipt" = 'private corrupt receipt'"#,
    ] {
        sqlx::query(statement).execute(&repo.0).await.unwrap();
        let error = repo.export(receipt.id, 1024, 1024).await.unwrap_err();
        assert!(matches!(error, RepositoryError::Decode { .. }));
        assert!(!error.to_string().contains("private"));
        restore_receipt(&repo, &receipt).await;
    }
    assert_eq!(count(&repo).await, 1);
    assert_eq!(repo.head(head.id).await.unwrap(), Some(head));
}

async fn restore_receipt(
    repo: &SqliteTeletextDesignRepository,
    receipt: &TeletextDesignExportMetadata,
) {
    sqlx::query(
        r#"UPDATE "TeletextDesignExportReceipt" SET "DesignId"=?, "Revision"=?, "DesignDigest"=?,
        "ExpectedDesignHead"=?, "ReceiptVersion"=1, "Digest"=?, "Receipt"=? WHERE "Id"=?"#,
    )
    .bind(receipt.design_id)
    .bind(receipt.revision)
    .bind(&receipt.design_digest)
    .bind(receipt.expected_design_head)
    .bind(&receipt.digest)
    .bind(RECEIPT)
    .bind(receipt.id)
    .execute(&repo.0)
    .await
    .unwrap();
}

#[tokio::test]
async fn export_creation_and_read_recheck_original_digest_and_version() {
    let (_directory, repo) = fixture().await;
    let head = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let receipt = exported(
        repo.create_export(&export_request(&head), 1024, 1024)
            .await
            .unwrap(),
    );
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
        .execute(&repo.0)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&repo.0)
        .await
        .unwrap();
    for statement in [
        r#"UPDATE "TeletextDesignRevision" SET "Digest" = printf('%064d', 0)"#,
        r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion" = 2"#,
        r#"UPDATE "TeletextDesignRevision" SET "Document" = X'00'"#,
    ] {
        sqlx::query(statement).execute(&repo.0).await.unwrap();
        assert!(matches!(
            repo.export(receipt.id, 1024, 1024).await,
            Err(RepositoryError::Decode { .. })
        ));
        assert!(matches!(
            repo.create_export(&export_request(&head), 1024, 1024).await,
            Err(RepositoryError::Decode { .. })
        ));
        sqlx::query(
            r#"UPDATE "TeletextDesignRevision" SET "Digest"=?, "DocumentVersion"=1, "Document"=?"#,
        )
        .bind(sha256_hex(DOCUMENT))
        .bind(DOCUMENT)
        .execute(&repo.0)
        .await
        .unwrap();
    }
    assert_eq!(count(&repo).await, 1);
    assert_eq!(repo.head(head.id).await.unwrap(), Some(head));
}
