use super::*;
use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
use std::time::Duration;

fn inputs() -> (TeletextDesignRevision, TeletextDesignExportSave) {
    let document = b"synthetic privately compiled original".to_vec();
    let original = TeletextDesignRevision {
        design_id: 7,
        revision: 2,
        document_version: TELETEXT_DESIGN_DOCUMENT_VERSION,
        digest: digest(&document),
        document,
    };
    let export = TeletextDesignExportSave {
        design_id: 7,
        revision: 2,
        design_digest: original.digest.clone(),
        expected_design_head: 3,
        receipt_version: TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION,
        receipt: b"synthetic privately mapped receipt".to_vec(),
    };
    (original, export)
}

fn confirmation(
    original: &TeletextDesignRevision,
    export: &TeletextDesignExportSave,
    hold: TeletextWorkHold,
) -> NativeExportConfirmation {
    NativeExportConfirmation {
        original: TeletextExportOriginal {
            design_id: original.design_id,
            revision: original.revision,
            digest: original.digest.clone(),
            expected_head: export.expected_design_head,
            document_version: original.document_version,
        },
        receipt_digest: digest(&export.receipt),
        max_document_bytes: original.document.len(),
        max_receipt_bytes: export.receipt.len(),
        deadline: Instant::now() + Duration::from_secs(30),
        hold,
    }
}

#[tokio::test]
async fn private_confirmation_checks_both_actual_bodies_not_claimed_digests() {
    let guard = TeletextWorkGuard::try_acquire(&EntityLocker::default(), 1).unwrap();
    let (original, export) = inputs();
    let confirmation = confirmation(&original, &export, guard.hold_work());
    confirmation
        .confirm_export(&original, &export)
        .await
        .unwrap();
    let mut changed = original.clone();
    changed.document[0] ^= 1;
    assert_eq!(
        confirmation.confirm_export(&changed, &export).await,
        Err(TeletextDesignExportValidationError::Association)
    );
    let mut changed = export.clone();
    changed.receipt[0] ^= 1;
    assert_eq!(
        confirmation.confirm_export(&original, &changed).await,
        Err(TeletextDesignExportValidationError::Association)
    );
}

#[tokio::test]
async fn private_confirmation_rejects_identity_versions_head_bounds_and_expiry() {
    let guard = TeletextWorkGuard::try_acquire(&EntityLocker::default(), 1).unwrap();
    let (original, export) = inputs();
    let mut confirmation = confirmation(&original, &export, guard.hold_work());
    for field in 0..4 {
        let mut changed = original.clone();
        match field {
            0 => changed.design_id += 1,
            1 => changed.revision += 1,
            2 => changed.document_version += 1,
            _ => changed.document.push(0),
        }
        assert_eq!(
            confirmation.confirm_export(&changed, &export).await,
            Err(TeletextDesignExportValidationError::Association)
        );
    }
    for field in 0..6 {
        let mut changed = export.clone();
        match field {
            0 => changed.design_id += 1,
            1 => changed.revision += 1,
            2 => changed.design_digest.replace_range(0..1, "z"),
            3 => changed.expected_design_head += 1,
            4 => changed.receipt_version += 1,
            _ => changed.receipt.push(0),
        }
        assert_eq!(
            confirmation.confirm_export(&original, &changed).await,
            Err(TeletextDesignExportValidationError::Association)
        );
    }
    confirmation.deadline = Instant::now();
    assert_eq!(
        confirmation.confirm_export(&original, &export).await,
        Err(TeletextDesignExportValidationError::TimedOut)
    );
}
