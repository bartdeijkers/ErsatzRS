//! Required application confirmation before opaque export persistence.
//! The synthetic validator is a witness/negative control, not a TTI exporter.

use super::{designs, generations, *};
use async_trait::async_trait;
use ersatzrs_app::teletext_design_exports::{
    TeletextDesignExportStoreError, TeletextDesignExportValidationError,
    TeletextDesignExportValidator,
};
use ersatzrs_core::repositories::{
    TeletextDesignExportSave, TeletextDesignMutation as Mutation,
    TeletextDesignRejection as Rejection, TeletextDesignRepository as _, TeletextDesignSave,
};
use ersatzrs_domain::{TeletextDesignExportMetadata, TeletextDesignRevision};
use ersatzrs_infra::repositories::SqliteTeletextDesignRepository;
use std::sync::atomic::{AtomicUsize, Ordering};

const RECEIPT: &[u8] = b"synthetic opaque export baseline, not an editor sidecar";

struct Confirmation {
    original: TeletextDesignRevision,
    denied: bool,
    change_head: Option<SqliteTeletextDesignRepository>,
    calls: AtomicUsize,
}

#[async_trait]
impl TeletextDesignExportValidator for Confirmation {
    async fn confirm_export(
        &self,
        original: &TeletextDesignRevision,
        export: &TeletextDesignExportSave,
    ) -> Result<(), TeletextDesignExportValidationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(original, &self.original);
        assert_eq!(export.design_id, original.design_id);
        assert_eq!(export.revision, original.revision);
        assert_eq!(export.design_digest, original.digest);
        assert_eq!(export.receipt, RECEIPT);
        if self.denied {
            return Err(TeletextDesignExportValidationError::Association);
        }
        if let Some(repository) = &self.change_head {
            let changed = repository
                .save(
                    &TeletextDesignSave {
                        name: "Synthetic concurrently saved original".into(),
                        document_version: original.document_version,
                        document: original.document.clone(),
                        existing: Some((original.design_id, export.expected_design_head)),
                    },
                    designs::budget().max_serialized_bytes,
                )
                .await
                .unwrap();
            assert!(matches!(changed, Mutation::Saved(_)));
        }
        Ok(())
    }
}

async fn source(
    fixture: &Fixture,
    saved: &ersatzrs_web::host::teletext_designs::SavedOriginalTeletextDesign,
) -> TeletextDesignRevision {
    fixture
        .state
        .application
        .teletext_design_revision(
            saved.head().id,
            saved.head().revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap()
}

fn export(original: &TeletextDesignRevision) -> TeletextDesignExportSave {
    TeletextDesignExportSave {
        design_id: original.design_id,
        revision: original.revision,
        design_digest: original.digest.clone(),
        expected_design_head: original.revision,
        receipt_version: 1,
        receipt: RECEIPT.to_vec(),
    }
}

fn validator(original: TeletextDesignRevision) -> Confirmation {
    Confirmation {
        original,
        denied: false,
        change_head: None,
        calls: AtomicUsize::new(0),
    }
}

async fn create(
    fixture: &Fixture,
    export: TeletextDesignExportSave,
    validator: &Confirmation,
) -> Result<Mutation<TeletextDesignExportMetadata>, TeletextDesignExportStoreError> {
    fixture
        .state
        .application
        .create_teletext_design_export(
            export,
            validator,
            1024,
            designs::budget().max_serialized_bytes,
        )
        .await
}

async fn receipts(fixture: &Fixture) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignExportReceipt""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn rejected_export_confirmation_preserves_nonempty_publication_and_original() {
    let (fixture, saved, pending) = generations::prepared().await;
    let accepted = generations::accepted(generations::accept(&fixture, &pending, 1, None).await);
    let original = source(&fixture, &saved).await;
    let configuration = designs::config(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("channel-calls")).unwrap();
    let mut confirmation = validator(original.clone());
    confirmation.denied = true;
    assert!(matches!(
        create(&fixture, export(&original), &confirmation).await,
        Err(TeletextDesignExportStoreError::Validation(
            TeletextDesignExportValidationError::Association
        ))
    ));
    assert_eq!(confirmation.calls.load(Ordering::SeqCst), 1);
    assert_eq!(receipts(&fixture).await, 0);
    assert_eq!(source(&fixture, &saved).await, original);
    generations::assert_same_output(&generations::recover(&fixture).await.unwrap(), &accepted);
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        std::fs::read(fixture.root.path().join("channel-calls")).unwrap(),
        calls
    );
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn export_shape_and_source_digest_fail_before_native_confirmation() {
    let (fixture, saved, _) = generations::prepared().await;
    let original = source(&fixture, &saved).await;
    let confirmation = validator(original.clone());
    let mut oversized = export(&original);
    oversized.receipt.resize(1025, b'x');
    assert!(matches!(
        create(&fixture, oversized, &confirmation).await.unwrap(),
        Mutation::Rejected(Rejection::InvalidValues)
    ));
    let mut wrong = export(&original);
    wrong.design_digest = "f".repeat(64);
    assert!(matches!(
        create(&fixture, wrong, &confirmation).await.unwrap(),
        Mutation::Rejected(Rejection::Conflict)
    ));
    assert_eq!(confirmation.calls.load(Ordering::SeqCst), 0);
    assert_eq!(receipts(&fixture).await, 0);
    assert_eq!(source(&fixture, &saved).await, original);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn confirmed_export_survives_reopen_without_changing_accepted_output() {
    let (mut fixture, saved, pending) = generations::prepared().await;
    let accepted = generations::accepted(generations::accept(&fixture, &pending, 1, None).await);
    let original = source(&fixture, &saved).await;
    let confirmation = validator(original.clone());
    let metadata = match create(&fixture, export(&original), &confirmation)
        .await
        .unwrap()
    {
        Mutation::Saved(value) => value,
        Mutation::Rejected(reason) => panic!("unexpected export rejection: {reason:?}"),
    };
    assert_eq!(confirmation.calls.load(Ordering::SeqCst), 1);
    assert_eq!(metadata.revision, 1);
    assert_eq!(metadata.expected_design_head, 1);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(original.design_id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    fixture = fixture.reopen().await;
    let retained = fixture
        .state
        .application
        .teletext_design_export_receipt(metadata.id, 1024, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retained.metadata, metadata);
    assert_eq!(retained.receipt, RECEIPT);
    assert_eq!(source(&fixture, &saved).await, original);
    generations::assert_same_output(&generations::recover(&fixture).await.unwrap(), &accepted);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn concurrent_head_change_during_confirmation_cannot_persist_export() {
    let (fixture, saved, pending) = generations::prepared().await;
    let accepted = generations::accepted(generations::accept(&fixture, &pending, 1, None).await);
    let original = source(&fixture, &saved).await;
    let mut confirmation = validator(original.clone());
    confirmation.change_head = Some(SqliteTeletextDesignRepository(fixture.state.pool.clone()));
    assert!(matches!(
        create(&fixture, export(&original), &confirmation)
            .await
            .unwrap(),
        Mutation::Rejected(Rejection::Conflict)
    ));
    assert_eq!(confirmation.calls.load(Ordering::SeqCst), 1);
    assert_eq!(receipts(&fixture).await, 0);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(original.design_id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    assert_eq!(source(&fixture, &saved).await, original);
    generations::assert_same_output(&generations::recover(&fixture).await.unwrap(), &accepted);
    fixture.state.pool.close().await;
}
