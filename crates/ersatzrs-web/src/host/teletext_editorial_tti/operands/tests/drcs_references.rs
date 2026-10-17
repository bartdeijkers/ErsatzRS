//! Only original final DRCS glyph words can change their checked PTU slot.
use super::{operand, TeletextColumnOperation};
use ersatzrs_domain::{
    TeletextDrcsGlyphReference, TeletextDrcsGlyphSlot, TeletextDrcsSubtableIndex,
    TeletextDrcsTableScope,
};

fn operation(table: TeletextDrcsTableScope, subtable: u8, slot: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Drcs {
        reference: TeletextDrcsGlyphReference {
            table,
            subtable: TeletextDrcsSubtableIndex::new(subtable).unwrap(),
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        },
    }
}
fn word(column: u8, table: TeletextDrcsTableScope, slot: u8) -> u32 {
    let scope = match table {
        TeletextDrcsTableScope::Normal => 0x40,
        TeletextDrcsTableScope::Global => 0,
    };
    u32::from(column) | (13 << 6) | (u32::from(scope | slot) << 11)
}

#[test]
fn all_normal_global_slots_keep_the_original_logical_subtable_and_scope() {
    for table in [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ] {
        for subtable in 0..16 {
            let original = operation(table, subtable, 0);
            for slot in 0..48 {
                assert_eq!(
                    operand(7, original, word(7, table, 0), word(7, table, slot)),
                    Some(operation(table, subtable, slot))
                );
            }
        }
    }
}

#[test]
fn drcs_slot_edits_refuse_scope_mode_address_high_bits_and_unchecked_ptus() {
    for table in [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ] {
        let original = operation(table, 15, 0);
        let before = word(39, table, 0);
        let edited = word(39, table, 47);
        for after in [
            edited ^ (0x40 << 11),
            edited ^ (1 << 6),
            edited ^ 1,
            edited | (1 << 18),
            word(39, table, 48),
            word(39, table, 63),
        ] {
            assert_eq!(operand(39, original, before, after), None);
        }
        assert_eq!(operand(39, operation(table, 15, 1), before, edited), None);
        assert_eq!(
            operand(40, original, word(40, table, 0), word(40, table, 1)),
            None
        );
        assert_eq!(
            operand(39, original, before, 0x28 | (24 << 6) | (0x50 << 11)),
            None
        );
    }
}
