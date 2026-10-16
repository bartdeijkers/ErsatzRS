use super::*;
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextDrcsBitmap, TeletextDrcsGlyphSlot,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex,
};
use std::time::Duration;

fn budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 4,
        max_objects: 4,
        max_structure_items: 64,
        max_base_content_bytes: 1600,
        max_commands: 8,
        max_collected_commands: 8,
        max_collected_drcs_references: 8,
    }
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn operand(page: u16, command: usize, old: u8, new: u8) -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(
        TeletextPageAddress {
            page: TeletextPageNumber::new(page).unwrap(),
            subcode: TeletextSubcode::new(16).unwrap(),
        },
        command,
        8,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(old).unwrap(),
        },
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(new).unwrap(),
        },
    )
    .unwrap()
}

#[test]
fn repeat_operands_keep_root_expectation_and_unselected_owners() {
    let prior = [operand(123, 2, 1, 4), operand(124, 3, 2, 5)];
    let incoming = [operand(123, 2, 4, 6), operand(125, 1, 0, 3)];
    let merged = merge_operands(&prior, &incoming, budget(), deadline()).unwrap();
    assert_eq!(merged, [operand(123, 2, 1, 6), prior[1], incoming[1]]);
}

#[test]
fn repeat_operand_revert_removes_only_exact_root_relative_target() {
    let prior = [operand(123, 2, 1, 4), operand(124, 3, 2, 5)];
    assert_eq!(
        merge_operands(&prior, &[operand(123, 2, 4, 1)], budget(), deadline()).unwrap(),
        [prior[1]],
    );
    assert!(
        merge_operands(&prior[..1], &[operand(123, 2, 4, 1)], budget(), deadline())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn repeat_operand_stale_and_repeated_targets_are_rejected_including_revert() {
    let prior = operand(123, 2, 1, 4);
    for incoming in [
        vec![operand(123, 2, 3, 6)],
        vec![operand(123, 2, 4, 1), operand(123, 2, 1, 6)],
        vec![operand(124, 2, 3, 6), operand(124, 2, 3, 6)],
    ] {
        assert!(matches!(
            merge_operands(&[prior], &incoming, budget(), deadline()),
            Err(NativeTeletextEditorialImportError::Association)
        ));
    }
    assert!(matches!(
        merge_operands(&[prior, prior], &[], budget(), deadline()),
        Err(NativeTeletextEditorialImportError::Association)
    ));
    let altered_column = TeletextLocalProgramOperandOverlay::new(
        prior.owner(),
        prior.command(),
        9,
        prior.replacement(),
        operand(123, 2, 4, 6).replacement(),
    )
    .unwrap();
    assert!(matches!(
        merge_operands(&[prior], &[altered_column], budget(), deadline()),
        Err(NativeTeletextEditorialImportError::Association)
    ));
}

#[test]
fn repeat_operand_union_and_work_reuse_existing_budget_and_deadline() {
    let prior = [operand(123, 2, 1, 4)];
    let incoming = [operand(124, 2, 1, 4)];
    let tight = TeletextServiceBudget {
        max_commands: 1,
        ..budget()
    };
    assert!(matches!(
        merge_operands(&prior, &incoming, tight, deadline()),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
    // A replacement of an existing target consumes no additional target slot.
    assert_eq!(
        merge_operands(&prior, &[operand(123, 2, 4, 6)], tight, deadline()).unwrap(),
        [operand(123, 2, 1, 6)]
    );
    assert!(matches!(
        merge_operands(&prior, &[], budget(), Instant::now()),
        Err(NativeTeletextEditorialImportError::TimedOut)
    ));
}

fn bitmap_edit(page: u8, s1: u8, slot: u8, old: u16, new: u16) -> TeletextDrcsBitmapOverlay {
    let bitmap = |bits| {
        let mut rows = [0; 10];
        rows[0] = bits;
        TeletextDrcsBitmap::new(rows).unwrap()
    };
    TeletextDrcsBitmapOverlay::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Drcs,
            TeletextRawDataPageNumber::new(1, page).unwrap(),
            TeletextResourceSubpageIndex::new(s1).unwrap(),
        )
        .unwrap(),
        TeletextDrcsGlyphSlot::new(slot).unwrap(),
        bitmap(old),
        bitmap(new),
    )
    .unwrap()
}

#[test]
fn repeat_bitmaps_keep_root_expectation_and_other_resources_subpages_and_slots() {
    let prior = [
        bitmap_edit(0xc0, 0, 0, 1, 3),
        bitmap_edit(0xc0, 1, 0, 4, 6),
        bitmap_edit(0xc0, 1, 1, 8, 10),
        bitmap_edit(0xc1, 1, 0, 16, 18),
    ];
    let incoming = [bitmap_edit(0xc0, 1, 0, 6, 7), bitmap_edit(0xc2, 0, 0, 1, 2)];
    let merged = merge_bitmaps(&prior, &incoming, budget(), deadline()).unwrap();
    assert_eq!(
        merged,
        [
            prior[0],
            bitmap_edit(0xc0, 1, 0, 4, 7),
            prior[2],
            prior[3],
            incoming[1]
        ]
    );
}

#[test]
fn repeat_bitmap_revert_removes_only_exact_root_relative_glyph() {
    let prior = [bitmap_edit(0xc0, 1, 0, 1, 3), bitmap_edit(0xc0, 1, 1, 4, 6)];
    let undo = bitmap_edit(0xc0, 1, 0, 3, 1);
    assert_eq!(
        merge_bitmaps(&prior, &[undo], budget(), deadline()).unwrap(),
        [prior[1]]
    );
    assert!(merge_bitmaps(&prior[..1], &[undo], budget(), deadline())
        .unwrap()
        .is_empty());
}

#[test]
fn repeat_bitmaps_reject_stale_and_duplicate_targets_even_after_revert() {
    let prior = bitmap_edit(0xc0, 1, 0, 1, 3);
    for incoming in [
        vec![bitmap_edit(0xc0, 1, 0, 2, 4)],
        vec![bitmap_edit(0xc0, 1, 0, 3, 1), bitmap_edit(0xc0, 1, 0, 1, 4)],
        vec![bitmap_edit(0xc1, 1, 0, 1, 3), bitmap_edit(0xc1, 1, 0, 1, 3)],
    ] {
        assert!(matches!(
            merge_bitmaps(&[prior], &incoming, budget(), deadline()),
            Err(NativeTeletextEditorialImportError::Association)
        ));
    }
    assert!(matches!(
        merge_bitmaps(&[prior, prior], &[], budget(), deadline()),
        Err(NativeTeletextEditorialImportError::Association)
    ));
}

#[test]
fn repeat_bitmaps_use_existing_structure_budget_and_deadline() {
    let prior = bitmap_edit(0xc0, 1, 0, 1, 3);
    let tight = TeletextServiceBudget {
        max_structure_items: 1,
        ..budget()
    };
    assert!(matches!(
        merge_bitmaps(
            &[prior],
            &[bitmap_edit(0xc0, 1, 1, 1, 3)],
            tight,
            deadline()
        ),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
    assert_eq!(
        merge_bitmaps(
            &[prior],
            &[bitmap_edit(0xc0, 1, 0, 3, 7)],
            tight,
            deadline()
        )
        .unwrap(),
        [bitmap_edit(0xc0, 1, 0, 1, 7)]
    );
    assert!(matches!(
        merge_bitmaps(&[prior], &[], budget(), Instant::now()),
        Err(NativeTeletextEditorialImportError::TimedOut)
    ));
    assert!(matches!(
        merge_bitmaps(
            &[prior],
            &[],
            TeletextServiceBudget {
                max_structure_items: 0,
                ..budget()
            },
            deadline()
        ),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
}
