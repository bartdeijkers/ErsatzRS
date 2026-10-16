//! Root-relative merging; original catalogue authentication remains external.
use super::{
    merge, NativeTeletextEditorialImportError, TeletextLocalObjectOperandOverlay,
    TeletextServiceBudget,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextGlyphCode, TeletextObjectId, TeletextObjectIdentity,
    TeletextPageAddress, TeletextPageNumber, TeletextSubcode,
};
use std::time::{Duration, Instant};

fn g0(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn change(id: u16, column: u8, old: u8, new: u8) -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(
        TeletextObjectIdentity::local(
            TeletextPageAddress {
                page: TeletextPageNumber::new(124).unwrap(),
                subcode: TeletextSubcode::new(32).unwrap(),
            },
            TeletextObjectId::new(id),
        ),
        0,
        column,
        g0(old),
        g0(new),
    )
    .unwrap()
}
fn budget(maximum: usize) -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 8,
        max_objects: 8,
        max_structure_items: maximum,
        max_base_content_bytes: 4096,
        max_commands: maximum,
        max_collected_commands: 64,
        max_collected_drcs_references: 24,
    }
}
fn deadline() -> Instant {
    Instant::now().checked_add(Duration::from_secs(30)).unwrap()
}

#[test]
fn cumulative_repeat_and_undo_keep_exact_root_expectations_and_other_objects() {
    let first = change(65520, 0, b'A', b'B');
    let other = change(65521, 0, b'X', b'Y');
    let initial = merge(&[], &[first, other], budget(2), deadline()).unwrap();
    let repeated = merge(
        &initial,
        &[change(65520, 0, b'B', b'C')],
        budget(2),
        deadline(),
    )
    .unwrap();
    assert_eq!(repeated, vec![change(65520, 0, b'A', b'C'), other]);
    // Returning to an intermediate value remains a root-relative change.
    let intermediate = merge(
        &repeated,
        &[change(65520, 0, b'C', b'B')],
        budget(2),
        deadline(),
    )
    .unwrap();
    assert_eq!(intermediate, initial);
    let undone = merge(
        &repeated,
        &[change(65520, 0, b'C', b'A')],
        budget(2),
        deadline(),
    )
    .unwrap();
    assert_eq!(undone, vec![other]);
    assert!(merge(
        &undone,
        &[change(65521, 0, b'Y', b'X')],
        budget(2),
        deadline()
    )
    .unwrap()
    .is_empty());
}

#[test]
fn stale_expectation_column_mismatch_and_duplicates_fail_closed() {
    let first = change(65520, 0, b'A', b'B');
    for incoming in [
        vec![change(65520, 0, b'A', b'C')],
        vec![change(65520, 1, b'B', b'C')],
        vec![change(65520, 0, b'B', b'C'), change(65520, 0, b'B', b'D')],
    ] {
        assert!(matches!(
            merge(&[first], &incoming, budget(2), deadline()),
            Err(NativeTeletextEditorialImportError::Association)
        ));
    }
    assert!(matches!(
        merge(&[first, first], &[], budget(2), deadline()),
        Err(NativeTeletextEditorialImportError::Association)
    ));
}

#[test]
fn exact_limits_and_both_independent_budget_caps_are_enforced() {
    let first = change(65520, 0, b'A', b'B');
    let other = change(65521, 0, b'X', b'Y');
    assert_eq!(
        merge(&[first], &[other], budget(2), deadline()).unwrap(),
        vec![first, other]
    );
    assert!(matches!(
        merge(&[first], &[other], budget(1), deadline()),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
    for limited in [
        TeletextServiceBudget {
            max_commands: 1,
            ..budget(2)
        },
        TeletextServiceBudget {
            max_structure_items: 1,
            ..budget(2)
        },
    ] {
        assert!(matches!(
            merge(&[], &[first, other], limited, deadline()),
            Err(NativeTeletextEditorialImportError::Budget)
        ));
        assert!(matches!(
            merge(&[first, other], &[], limited, deadline()),
            Err(NativeTeletextEditorialImportError::Budget)
        ));
    }
}

#[test]
fn expired_previous_and_incoming_scans_cannot_return_an_authorized_merge() {
    let first = change(65520, 0, b'A', b'B');
    assert!(matches!(
        merge(&[first], &[], budget(1), Instant::now()),
        Err(NativeTeletextEditorialImportError::TimedOut)
    ));
    assert!(matches!(
        merge(&[], &[first], budget(1), Instant::now()),
        Err(NativeTeletextEditorialImportError::TimedOut)
    ));
}
