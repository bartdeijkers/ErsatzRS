//! Opaque format admission preserves integrity and grants no base authority.

use super::*;

const EFFECTIVE: &[u8] = b"{\"format_version\":2,\"effective_selection\":{\"channel_id\":1,\"bindings\":[]},\"producer_bindings\":[]}\r\n";

fn effective(existing: Option<(i64, i64)>) -> TeletextDesignSave {
    TeletextDesignSave {
        name: "Synthetic effective originals".to_owned(),
        document_version: TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
        document: EFFECTIVE.to_vec(),
        existing,
    }
}

async fn assert_no_authority(repo: &SqliteTeletextDesignRepository) {
    for query in [
        r#"SELECT COUNT(*) FROM "TeletextDesignExportReceipt""#,
        r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#,
        r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#,
        r#"SELECT COUNT(*) FROM "ChannelTeletextBinding""#,
    ] {
        let count: i64 = sqlx::query_scalar(query).fetch_one(&repo.0).await.unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn effective_and_legacy_exact_revisions_reopen_and_tombstone_without_authority() {
    let (directory, repo) = fixture().await;
    let legacy = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let original = saved(repo.save(&effective(None), EFFECTIVE.len()).await.unwrap());
    let mut edit = effective(Some((original.id, original.revision)));
    edit.document.extend_from_slice(b" ");
    let latest = saved(repo.save(&edit, edit.document.len()).await.unwrap());
    assert_eq!(latest.revision, 2);
    assert_eq!(revisions(&repo).await, 3);
    repo.0.close().await;
    let repo = SqliteTeletextDesignRepository(open(&directory.path().join("designs.sqlite")).await);
    let stored = repo
        .revision(original.id, 1, EFFECTIVE.len())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.document, EFFECTIVE);
    assert_eq!(
        stored.digest,
        "116e1399a555cfc4759f4552d9935d4b47e9b2981f516eac72bc4444f2bd08b9"
    );
    assert_eq!(
        stored.document_version,
        TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION
    );
    let updated = repo
        .revision(original.id, 2, edit.document.len())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.document, edit.document);
    assert_eq!(
        updated.document_version,
        TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION
    );
    let old = repo
        .revision(legacy.id, 1, DOCUMENT.len())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old.document, DOCUMENT);
    assert_eq!(old.document_version, TELETEXT_DESIGN_DOCUMENT_VERSION);
    assert_eq!(
        old.digest,
        "bf963dbbcc648cd9be1b37a58c76bbd497cfb53a94176644ba8993501ae05cb7"
    );
    let deleted = saved(repo.tombstone(original.id, latest.revision).await.unwrap());
    assert!(deleted.deleted);
    assert_eq!(
        repo.save(&effective(Some((original.id, latest.revision))), 1024)
            .await
            .unwrap(),
        M::Rejected(R::DeletedDesign)
    );
    assert_eq!(
        repo.revision(original.id, 1, EFFECTIVE.len())
            .await
            .unwrap()
            .unwrap(),
        stored
    );
    assert_no_authority(&repo).await;
}

#[tokio::test]
async fn unsupported_versions_bounds_and_cas_fail_without_advancing_effective_head() {
    let (_directory, repo) = fixture().await;
    let first = saved(repo.save(&effective(None), EFFECTIVE.len()).await.unwrap());
    for version in [0, 3, u16::MAX] {
        let mut invalid = effective(Some((first.id, 1)));
        invalid.document_version = version;
        assert_eq!(
            repo.save(&invalid, 1024).await.unwrap(),
            M::Rejected(R::InvalidValues)
        );
    }
    assert_eq!(
        repo.save(&effective(None), EFFECTIVE.len() - 1)
            .await
            .unwrap(),
        M::Rejected(R::InvalidValues)
    );
    assert_eq!(
        repo.save(&effective(Some((first.id, 2))), 1024)
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    assert!(repo
        .revision(first.id, 1, EFFECTIVE.len() - 1)
        .await
        .is_err());
    sqlx::query(r#"CREATE TRIGGER "SyntheticEffectiveRevisionFailure" BEFORE INSERT ON "TeletextDesignRevision"
        BEGIN SELECT RAISE(ABORT, 'synthetic effective revision failure'); END"#)
        .execute(&repo.0).await.unwrap();
    assert!(repo.save(&effective(None), 1024).await.is_err());
    assert!(repo
        .save(&effective(Some((first.id, 1))), 1024)
        .await
        .is_err());
    assert_eq!(repo.head(first.id).await.unwrap(), Some(first));
    assert_eq!(repo.heads(None, 10).await.unwrap().len(), 1);
    assert_eq!(revisions(&repo).await, 1);
    assert_no_authority(&repo).await;
}

#[tokio::test]
async fn effective_digest_type_version_and_bounded_payload_corruption_is_not_absence() {
    let (_directory, repo) = fixture().await;
    let head = saved(repo.save(&effective(None), EFFECTIVE.len()).await.unwrap());
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
        .execute(&repo.0)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&repo.0)
        .await
        .unwrap();
    for mutation in [
        r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion"=3"#,
        r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion"=zeroblob(4096)"#,
        r#"UPDATE "TeletextDesignRevision" SET "Digest"=printf('%064d',0)"#,
        r#"UPDATE "TeletextDesignRevision" SET "Digest"=zeroblob(4096)"#,
        r#"UPDATE "TeletextDesignRevision" SET "Document"=zeroblob(2048)"#,
        r#"UPDATE "TeletextDesignRevision" SET "Document"='same-looking text is not a blob'"#,
    ] {
        sqlx::query(
            r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion"=2,"Digest"=?,"Document"=?"#,
        )
        .bind(sha256_hex(EFFECTIVE))
        .bind(EFFECTIVE)
        .execute(&repo.0)
        .await
        .unwrap();
        sqlx::query(mutation).execute(&repo.0).await.unwrap();
        assert!(matches!(
            repo.revision(head.id, 1, 1024).await,
            Err(RepositoryError::Decode { .. })
        ));
        assert_eq!(repo.head(head.id).await.unwrap(), Some(head.clone()));
        assert_eq!(revisions(&repo).await, 1);
    }
    assert_no_authority(&repo).await;
}

#[tokio::test]
async fn opaque_effective_design_cannot_become_a_base_publication_or_editor_export() {
    use crate::repositories::teletext_publications::tests::Fixture as PublicationFixture;
    use ersatzrs_core::repositories::TeletextPublicationRepository as _;

    let fixture = PublicationFixture::new().await;
    let designs = SqliteTeletextDesignRepository(fixture.repo.0.clone());
    let head = saved(
        designs
            .save(&effective(None), EFFECTIVE.len())
            .await
            .unwrap(),
    );
    let revision = designs
        .revision(head.id, 1, EFFECTIVE.len())
        .await
        .unwrap()
        .unwrap();
    let mut save = fixture.save(None);
    save.design_id = head.id;
    save.digest.clone_from(&revision.digest);
    assert!(matches!(
        fixture.repo.request(&save, 1024).await.unwrap(),
        ersatzrs_core::repositories::TeletextPublicationMutation::Rejected(_)
    ));
    let export = TeletextDesignExportSave {
        design_id: head.id,
        revision: 1,
        design_digest: revision.digest,
        expected_design_head: 1,
        receipt_version: 1,
        receipt: b"synthetic base editor receipt".to_vec(),
    };
    assert!(matches!(
        designs.create_export(&export, 1024, 1024).await.unwrap(),
        M::Rejected(R::InvalidValues)
    ));
    for query in [
        r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#,
        r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#,
        r#"SELECT COUNT(*) FROM "TeletextDesignExportReceipt""#,
    ] {
        let count: i64 = sqlx::query_scalar(query)
            .fetch_one(&designs.0)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    assert_eq!(designs.head(head.id).await.unwrap(), Some(head));
}

#[tokio::test]
async fn stored_base_export_rejects_an_original_redeclared_as_effective_after_reopen() {
    let (directory, repo) = fixture().await;
    let head = saved(repo.save(&request(None), DOCUMENT.len()).await.unwrap());
    let M::Saved(export) = repo
        .create_export(
            &TeletextDesignExportSave {
                design_id: head.id,
                revision: 1,
                design_digest: sha256_hex(DOCUMENT),
                expected_design_head: 1,
                receipt_version: 1,
                receipt: b"synthetic original export".to_vec(),
            },
            1024,
            1024,
        )
        .await
        .unwrap()
    else {
        panic!("legacy export rejected")
    };
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
        .execute(&repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion"=2"#)
        .execute(&repo.0)
        .await
        .unwrap();
    repo.0.close().await;
    let repo = SqliteTeletextDesignRepository(open(&directory.path().join("designs.sqlite")).await);
    // Both opaque formats can be read, but a base export cannot borrow version 2.
    assert_eq!(
        repo.revision(head.id, 1, 1024)
            .await
            .unwrap()
            .unwrap()
            .document_version,
        TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION
    );
    assert!(matches!(
        repo.export(export.id, 1024, 1024).await,
        Err(RepositoryError::Decode {
            kind: "teletext design storage",
            ..
        })
    ));
    let receipts: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignExportReceipt""#)
        .fetch_one(&repo.0)
        .await
        .unwrap();
    assert_eq!(receipts, 1);
}
